"""Consolidate adjacent runs that share identical formatting in DOCX files.

Fuses neighbouring <w:r> elements whose <w:rPr> properties match exactly.
Operates on runs within regular paragraphs as well as inside tracked-change
wrappers (<w:ins>, <w:del>).

Additionally:
- Strips rsid attributes from runs (revision metadata irrelevant to rendering)
- Removes proofErr markers (spell/grammar annotations that impede merging)
"""

import defusedxml.minidom
from pathlib import Path


def _locate_by_tag(root, tag_suffix: str) -> list:
    """Depth-first search for elements whose local name matches tag_suffix."""
    found = []

    def _walk(nd):
        if nd.nodeType == nd.ELEMENT_NODE:
            local = nd.localName or nd.tagName
            if local == tag_suffix or local.endswith(":%s" % tag_suffix):
                found.append(nd)
            for c in nd.childNodes:
                _walk(c)

    _walk(root)
    return found


def _first_child_by_tag(parent, tag_suffix: str):
    """Return the first direct child element matching tag_suffix, or None."""
    for c in parent.childNodes:
        if c.nodeType != c.ELEMENT_NODE:
            continue
        local = c.localName or c.tagName
        if local == tag_suffix or local.endswith(":%s" % tag_suffix):
            return c
    return None


def _children_by_tag(parent, tag_suffix: str) -> list:
    """Collect all direct child elements matching tag_suffix."""
    return [
        c for c in parent.childNodes
        if c.nodeType == c.ELEMENT_NODE
        and ((c.localName or c.tagName) == tag_suffix
             or (c.localName or c.tagName).endswith(":%s" % tag_suffix))
    ]


def _truly_adjacent(a, b) -> bool:
    """Check whether two sibling elements have nothing meaningful between them."""
    cursor = a.nextSibling
    while cursor is not None:
        if cursor is b:
            return True
        if cursor.nodeType == cursor.ELEMENT_NODE:
            return False
        if cursor.nodeType == cursor.TEXT_NODE and cursor.data.strip():
            return False
        cursor = cursor.nextSibling
    return False


def _strip_proof_markers(root):
    """Delete all proofErr elements from the tree."""
    for el in _locate_by_tag(root, "proofErr"):
        if el.parentNode:
            el.parentNode.removeChild(el)


def _strip_rsid_from_runs(root):
    """Remove rsid-related attributes from every run element."""
    for run in _locate_by_tag(root, "r"):
        for attr in list(run.attributes.values()):
            if "rsid" in attr.name.lower():
                run.removeAttribute(attr.name)


def _is_run_element(nd) -> bool:
    local = nd.localName or nd.tagName
    return local == "r" or local.endswith(":r")


def _formatting_matches(r1, r2) -> bool:
    """True when two runs share identical run-properties (or both lack them)."""
    props1 = _first_child_by_tag(r1, "rPr")
    props2 = _first_child_by_tag(r2, "rPr")
    if (props1 is None) != (props2 is None):
        return False
    if props1 is None:
        return True
    return props1.toxml() == props2.toxml()


def _transfer_content(dest, donor):
    """Move non-rPr children from donor into dest."""
    for ch in list(donor.childNodes):
        if ch.nodeType != ch.ELEMENT_NODE:
            continue
        local = ch.localName or ch.tagName
        if local == "rPr" or local.endswith(":rPr"):
            continue
        dest.appendChild(ch)


def _coalesce_text_nodes(run):
    """Merge adjacent <w:t> elements within a single run."""
    t_nodes = _children_by_tag(run, "t")

    idx = len(t_nodes) - 1
    while idx > 0:
        cur, prev = t_nodes[idx], t_nodes[idx - 1]

        if _truly_adjacent(prev, cur):
            ptxt = prev.firstChild.data if prev.firstChild else ""
            ctxt = cur.firstChild.data if cur.firstChild else ""
            combined = ptxt + ctxt

            if prev.firstChild:
                prev.firstChild.data = combined
            else:
                prev.appendChild(run.ownerDocument.createTextNode(combined))

            if combined[0:1] == " " or combined[-1:] == " ":
                prev.setAttribute("xml:space", "preserve")
            elif prev.hasAttribute("xml:space"):
                prev.removeAttribute("xml:space")

            run.removeChild(cur)

        idx -= 1


def _find_first_run(container):
    for ch in container.childNodes:
        if ch.nodeType == ch.ELEMENT_NODE and _is_run_element(ch):
            return ch
    return None


def _next_elem_sibling(nd):
    sib = nd.nextSibling
    while sib is not None:
        if sib.nodeType == sib.ELEMENT_NODE:
            return sib
        sib = sib.nextSibling
    return None


def _next_run_sibling(nd):
    sib = nd.nextSibling
    while sib is not None:
        if sib.nodeType == sib.ELEMENT_NODE and _is_run_element(sib):
            return sib
        sib = sib.nextSibling
    return None


def _fuse_runs_in_container(container) -> int:
    """Merge adjacent compatible runs within a single container; return count."""
    fused = 0
    current = _find_first_run(container)

    while current is not None:
        while True:
            nxt = _next_elem_sibling(current)
            if nxt and _is_run_element(nxt) and _formatting_matches(current, nxt):
                _transfer_content(current, nxt)
                container.removeChild(nxt)
                fused += 1
            else:
                break

        _coalesce_text_nodes(current)
        current = _next_run_sibling(current)

    return fused


def merge_runs(input_dir: str) -> tuple[int, str]:
    """Entry point: merge adjacent identically-formatted runs in document.xml."""
    doc_path = Path(input_dir) / "word" / "document.xml"

    if not doc_path.exists():
        return 0, "Error: %s not found" % doc_path

    try:
        dom = defusedxml.minidom.parseString(doc_path.read_text(encoding="utf-8"))
        top = dom.documentElement

        _strip_proof_markers(top)
        _strip_rsid_from_runs(top)

        parents = {r.parentNode for r in _locate_by_tag(top, "r")}

        total = 0
        for p in parents:
            total += _fuse_runs_in_container(p)

        doc_path.write_bytes(dom.toxml(encoding="UTF-8"))
        return total, "Merged %d runs" % total

    except Exception as exc:
        return 0, "Error: %s" % exc
