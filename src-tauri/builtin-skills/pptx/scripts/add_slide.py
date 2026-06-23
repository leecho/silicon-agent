"""Append a fresh slide to an extracted PPTX package.

Invocation: python add_slide.py <extracted_path> <origin>

The origin argument accepts:
  - An existing slide filename (e.g., slide2.xml) to duplicate it
  - A layout filename (e.g., slideLayout2.xml) to generate from that layout

Samples:
    python add_slide.py unpacked/ slide2.xml
    # Clones slide2, producing slide5.xml

    python add_slide.py unpacked/ slideLayout2.xml
    # Builds slide5.xml based on slideLayout2.xml

View all available layouts with: ls unpacked/ppt/slideLayouts/

Emits the <p:sldId> tag to insert into presentation.xml.
"""

import re
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

SLIDE_CONTENT_TYPE = (
    "application/vnd.openxmlformats-officedocument.presentationml.slide+xml"
)
SLIDE_RELATIONSHIP_TYPE = (
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide"
)
LAYOUT_RELATIONSHIP_TYPE = (
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout"
)
NOTES_SLIDE_RELATIONSHIP_TYPE = "notesSlide"

DEFAULT_SLIDE_ID_START = 256

BLANK_SLIDE_TEMPLATE = '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr>
        <a:xfrm>
          <a:off x="0" y="0"/>
          <a:ext cx="0" cy="0"/>
          <a:chOff x="0" y="0"/>
          <a:chExt cx="0" cy="0"/>
        </a:xfrm>
      </p:grpSpPr>
    </p:spTree>
  </p:cSld>
  <p:clrMapOvr>
    <a:masterClrMapping/>
  </p:clrMapOvr>
</p:sld>'''

RELS_WRAPPER = '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="{rel_type}" Target="../slideLayouts/{layout}"/>
</Relationships>'''

SLIDE_FILENAME_PATTERN = re.compile(r"slide(\d+)\.xml")
RID_PATTERN = re.compile(r'Id="rId(\d+)"')
SLIDE_ID_PATTERN = re.compile(r'<p:sldId[^>]*id="(\d+)"')
NOTES_REL_PATTERN = re.compile(
    r'\s*<Relationship[^>]*Type="[^"]*notesSlide"[^>]*/>\s*'
)


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass
class SlideCreationResult:
    """Holds the outcome of a slide creation operation."""

    slide_name: str
    source_name: str
    slide_id: int
    relationship_id: str


# ---------------------------------------------------------------------------
# Index & ID resolution
# ---------------------------------------------------------------------------


def compute_next_slide_index(slides_folder: Path) -> int:
    """Scan existing slides and return the next available numeric index."""
    indices: list[int] = []
    for fp in slides_folder.glob("slide*.xml"):
        match = SLIDE_FILENAME_PATTERN.match(fp.name)
        if match:
            indices.append(int(match.group(1)))
    return max(indices) + 1 if indices else 1


def determine_next_slide_id(pkg_root: Path) -> int:
    """Read presentation.xml and return max(sldId) + 1."""
    pres_xml = pkg_root / "ppt" / "presentation.xml"
    body = pres_xml.read_text(encoding="utf-8")
    ids = [int(v) for v in SLIDE_ID_PATTERN.findall(body)]
    return max(ids) + 1 if ids else DEFAULT_SLIDE_ID_START


# ---------------------------------------------------------------------------
# Registration helpers
# ---------------------------------------------------------------------------


def register_content_type(pkg_root: Path, slide_name: str) -> None:
    """Ensure the new slide has a ContentType Override entry."""
    ct_file = pkg_root / "[Content_Types].xml"
    ct_text = ct_file.read_text(encoding="utf-8")

    part_name = "/ppt/slides/%s" % slide_name
    if part_name in ct_text:
        return

    entry = (
        '<Override PartName="%s" ContentType="%s"/>'
        % (part_name, SLIDE_CONTENT_TYPE)
    )
    ct_text = ct_text.replace("</Types>", "  %s\n</Types>" % entry)
    ct_file.write_text(ct_text, encoding="utf-8")


def register_presentation_relationship(pkg_root: Path, slide_name: str) -> str:
    """Add a Relationship entry in presentation.xml.rels; return the new rId."""
    rel_file = pkg_root / "ppt" / "_rels" / "presentation.xml.rels"
    rel_text = rel_file.read_text(encoding="utf-8")

    existing_ids = [int(v) for v in RID_PATTERN.findall(rel_text)]
    new_id_num = max(existing_ids) + 1 if existing_ids else 1
    rid = "rId%d" % new_id_num

    target_path = "slides/%s" % slide_name
    if target_path in rel_text:
        return rid

    tag = (
        '<Relationship Id="{rid}" Type="{rel_type}" Target="{target}"/>'
    ).format(rid=rid, rel_type=SLIDE_RELATIONSHIP_TYPE, target=target_path)

    rel_text = rel_text.replace("</Relationships>", "  %s\n</Relationships>" % tag)
    rel_file.write_text(rel_text, encoding="utf-8")

    return rid


# ---------------------------------------------------------------------------
# Source classification
# ---------------------------------------------------------------------------


def is_layout_source(source_arg: str) -> bool:
    """Determine whether the source argument refers to a slide layout."""
    return source_arg.startswith("slideLayout") and source_arg.endswith(".xml")


# ---------------------------------------------------------------------------
# Core operations
# ---------------------------------------------------------------------------


def build_from_layout(pkg_dir: Path, layout_name: str) -> SlideCreationResult:
    """Generate a blank slide linked to the specified layout."""
    slides_folder = pkg_dir / "ppt" / "slides"
    rels_folder = slides_folder / "_rels"
    layouts_folder = pkg_dir / "ppt" / "slideLayouts"

    layout_fp = layouts_folder / layout_name
    if not layout_fp.exists():
        print("Error: %s not found" % layout_fp, file=sys.stderr)
        sys.exit(1)

    idx = compute_next_slide_index(slides_folder)
    target_name = "slide%d.xml" % idx
    target_fp = slides_folder / target_name
    target_rels = rels_folder / ("%s.rels" % target_name)

    # Write blank slide XML
    target_fp.write_text(BLANK_SLIDE_TEMPLATE, encoding="utf-8")

    # Write relationship file linking to the layout
    rels_folder.mkdir(exist_ok=True)
    target_rels.write_text(
        RELS_WRAPPER.format(rel_type=LAYOUT_RELATIONSHIP_TYPE, layout=layout_name),
        encoding="utf-8",
    )

    # Register in package metadata
    register_content_type(pkg_dir, target_name)
    rid = register_presentation_relationship(pkg_dir, target_name)
    sid = determine_next_slide_id(pkg_dir)

    return SlideCreationResult(
        slide_name=target_name,
        source_name=layout_name,
        slide_id=sid,
        relationship_id=rid,
    )


def clone_slide(pkg_dir: Path, origin: str) -> SlideCreationResult:
    """Duplicate an existing slide file."""
    slides_folder = pkg_dir / "ppt" / "slides"
    rels_folder = slides_folder / "_rels"

    origin_fp = slides_folder / origin
    if not origin_fp.exists():
        print("Error: %s not found" % origin_fp, file=sys.stderr)
        sys.exit(1)

    idx = compute_next_slide_index(slides_folder)
    target_name = "slide%d.xml" % idx
    target_fp = slides_folder / target_name

    origin_rels = rels_folder / ("%s.rels" % origin)
    target_rels = rels_folder / ("%s.rels" % target_name)

    # Copy slide content
    shutil.copy2(origin_fp, target_fp)

    # Copy and sanitize relationship file (strip notes-slide references)
    if origin_rels.exists():
        shutil.copy2(origin_rels, target_rels)
        rels_body = target_rels.read_text(encoding="utf-8")
        rels_body = NOTES_REL_PATTERN.sub("\n", rels_body)
        target_rels.write_text(rels_body, encoding="utf-8")

    # Register in package metadata
    register_content_type(pkg_dir, target_name)
    rid = register_presentation_relationship(pkg_dir, target_name)
    sid = determine_next_slide_id(pkg_dir)

    return SlideCreationResult(
        slide_name=target_name,
        source_name=origin,
        slide_id=sid,
        relationship_id=rid,
    )


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------


def main() -> None:
    """Parse arguments and dispatch to the appropriate creation function."""
    if len(sys.argv) != 3:
        print("Usage: python add_slide.py <unpacked_dir> <source>", file=sys.stderr)
        print("", file=sys.stderr)
        print("Source can be:", file=sys.stderr)
        print("  slide2.xml        - duplicate an existing slide", file=sys.stderr)
        print("  slideLayout2.xml  - create from a layout template", file=sys.stderr)
        print("", file=sys.stderr)
        print(
            "To see available layouts: ls <unpacked_dir>/ppt/slideLayouts/",
            file=sys.stderr,
        )
        sys.exit(1)

    pkg = Path(sys.argv[1])
    src_arg = sys.argv[2]

    if not pkg.exists():
        print("Error: %s not found" % pkg, file=sys.stderr)
        sys.exit(1)

    if is_layout_source(src_arg):
        result = build_from_layout(pkg, src_arg)
    else:
        result = clone_slide(pkg, src_arg)

    print("Created %s from %s" % (result.slide_name, result.source_name))
    print(
        'Add to presentation.xml <p:sldIdLst>: <p:sldId id="%d" r:id="%s"/>'
        % (result.slide_id, result.relationship_id)
    )


if __name__ == "__main__":
    main()
