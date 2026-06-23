"""Collapse sequential tracked changes by the same author.

Fuses neighbouring <w:ins> elements authored by the same person into one.
Applies the same logic to <w:del> elements.  This reduces the number of
tracked-change wrappers in heavily-edited documents, simplifying further
manipulation.

Constraints:
- Only merges elements of the same type (ins with ins, del with del)
- Requires matching author attributes (timestamp differences are ignored)
- Elements must be truly adjacent (only whitespace nodes between them)
"""

import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

import defusedxml.minidom

_WP_NAMESPACE = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"


def _scan_elements(root, tag_suffix: str) -> list:
    """Depth-first collection of elements matching a local tag name."""
    hits = []

    def _recurse(nd):
        if nd.nodeType == nd.ELEMENT_NODE:
            local = nd.localName or nd.tagName
            if local == tag_suffix or local.endswith(":%s" % tag_suffix):
                hits.append(nd)
            for ch in nd.childNodes:
                _recurse(ch)

    _recurse(root)
    return hits


def _matches_tag(nd, tag_suffix: str) -> bool:
    local = nd.localName or nd.tagName
    return local == tag_suffix or local.endswith(":%s" % tag_suffix)


def _extract_author(el) -> str:
    """Read the author attribute regardless of namespace prefix."""
    val = el.getAttribute("w:author")
    if val:
        return val
    for attr in el.attributes.values():
        if attr.localName == "author" or attr.name.endswith(":author"):
            return attr.value
    return ""


def _authors_match_and_adjacent(el1, el2) -> bool:
    """True when two tracked-change elements share an author and are neighbours."""
    if _extract_author(el1) != _extract_author(el2):
        return False
    cursor = el1.nextSibling
    while cursor is not None and cursor is not el2:
        if cursor.nodeType == cursor.ELEMENT_NODE:
            return False
        if cursor.nodeType == cursor.TEXT_NODE and cursor.data.strip():
            return False
        cursor = cursor.nextSibling
    return True


def _absorb_children(target, source):
    """Move all children from source into target."""
    while source.firstChild:
        nd = source.firstChild
        source.removeChild(nd)
        target.appendChild(nd)


def _collapse_tracked_in(container, tag_suffix: str) -> int:
    """Fuse adjacent tracked-change elements of a given type inside container."""
    tracked = [
        ch for ch in container.childNodes
        if ch.nodeType == ch.ELEMENT_NODE and _matches_tag(ch, tag_suffix)
    ]

    if len(tracked) < 2:
        return 0

    fused = 0
    pos = 0
    while pos < len(tracked) - 1:
        a, b = tracked[pos], tracked[pos + 1]
        if _authors_match_and_adjacent(a, b):
            _absorb_children(a, b)
            container.removeChild(b)
            tracked.pop(pos + 1)
            fused += 1
        else:
            pos += 1

    return fused


def simplify_redlines(input_dir: str) -> tuple[int, str]:
    """Entry point: collapse adjacent tracked changes in document.xml."""
    doc_path = Path(input_dir) / "word" / "document.xml"

    if not doc_path.exists():
        return 0, "Error: %s not found" % doc_path

    try:
        dom = defusedxml.minidom.parseString(doc_path.read_text(encoding="utf-8"))
        top = dom.documentElement

        scopes = _scan_elements(top, "p") + _scan_elements(top, "tc")

        total = 0
        for scope in scopes:
            total += _collapse_tracked_in(scope, "ins")
            total += _collapse_tracked_in(scope, "del")

        doc_path.write_bytes(dom.toxml(encoding="UTF-8"))
        return total, "Simplified %d tracked changes" % total

    except Exception as exc:
        return 0, "Error: %s" % exc


def get_tracked_change_authors(doc_xml_fp: Path) -> dict[str, int]:
    """Count tracked-change occurrences per author in an XML file."""
    if not doc_xml_fp.exists():
        return {}
    try:
        tree = ET.parse(doc_xml_fp)
        root = tree.getroot()
    except ET.ParseError:
        return {}

    ns = {"w": _WP_NAMESPACE}
    attr_key = "{%s}author" % _WP_NAMESPACE

    counts: dict[str, int] = {}
    for kind in ("ins", "del"):
        for el in root.findall(".//w:%s" % kind, ns):
            who = el.get(attr_key)
            if who:
                counts[who] = counts.get(who, 0) + 1
    return counts


def _authors_from_packaged_docx(docx_fp: Path) -> dict[str, int]:
    """Extract author counts directly from a packaged .docx."""
    try:
        with zipfile.ZipFile(docx_fp, "r") as zf:
            if "word/document.xml" not in zf.namelist():
                return {}
            with zf.open("word/document.xml") as fh:
                tree = ET.parse(fh)
                root = tree.getroot()
                ns = {"w": _WP_NAMESPACE}
                attr_key = "{%s}author" % _WP_NAMESPACE
                counts: dict[str, int] = {}
                for kind in ("ins", "del"):
                    for el in root.findall(".//w:%s" % kind, ns):
                        who = el.get(attr_key)
                        if who:
                            counts[who] = counts.get(who, 0) + 1
                return counts
    except (zipfile.BadZipFile, ET.ParseError):
        return {}


def infer_author(modified_dir: Path, original_docx: Path, default: str = "Claude") -> str:
    """Determine which author introduced new tracked changes."""
    mod_xml = modified_dir / "word" / "document.xml"
    mod_counts = get_tracked_change_authors(mod_xml)

    if not mod_counts:
        return default

    orig_counts = _authors_from_packaged_docx(original_docx)

    deltas: dict[str, int] = {}
    for who, n in mod_counts.items():
        diff = n - orig_counts.get(who, 0)
        if diff > 0:
            deltas[who] = diff

    if not deltas:
        return default

    if len(deltas) == 1:
        return next(iter(deltas))

    raise ValueError(
        "Multiple authors added new changes: %s. "
        "Cannot infer which author to validate." % deltas
    )
