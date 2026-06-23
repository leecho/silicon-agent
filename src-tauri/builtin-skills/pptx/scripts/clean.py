"""Purge orphaned resources from an extracted PPTX package.

Invocation: python clean.py <extracted_path>

Sample:
    python clean.py unpacked/

Eliminates:
- Slides absent from sldIdLst together with their relationship files
- The [trash] staging area (leftover unreferenced assets)
- Stale .rels entries for removed resources
- Dangling media, embeddings, charts, diagrams, drawings, and ink artifacts
- Unreferenced theme definitions
- Orphaned notes slide files
- ContentType overrides pointing to deleted resources
"""

import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

import defusedxml.minidom

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

RESOURCE_CATEGORIES: list[str] = [
    "media", "embeddings", "charts", "diagrams", "tags", "drawings", "ink"
]
CHART_LIKE_DIRS: list[str] = ["charts", "diagrams", "drawings"]

SLIDE_ID_RID_PATTERN = re.compile(r'<p:sldId[^>]*r:id="([^"]+)"')
RELATIONSHIP_TAG = "Relationship"
OVERRIDE_TAG = "Override"

TRASH_DIR_NAME = "[trash]"


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass
class CleanupReport:
    """Tracks all files removed during the cleanup process."""

    removed_paths: list[str] = field(default_factory=list)

    @property
    def count(self) -> int:
        return len(self.removed_paths)

    def extend(self, paths: list[str]) -> None:
        self.removed_paths.extend(paths)


# ---------------------------------------------------------------------------
# Active slide resolution
# ---------------------------------------------------------------------------


def resolve_active_slides(pkg_root: Path) -> set[str]:
    """Return the set of slide filenames currently listed in sldIdLst."""
    pres_xml = pkg_root / "ppt" / "presentation.xml"
    pres_rels = pkg_root / "ppt" / "_rels" / "presentation.xml.rels"

    if not pres_xml.exists() or not pres_rels.exists():
        return set()

    # Build rId → slide filename mapping from rels
    dom = defusedxml.minidom.parse(str(pres_rels))
    rid_to_slide: dict[str, str] = {}
    for node in dom.getElementsByTagName(RELATIONSHIP_TAG):
        r_id = node.getAttribute("Id")
        tgt = node.getAttribute("Target")
        rtype = node.getAttribute("Type")
        if "slide" in rtype and tgt.startswith("slides/"):
            rid_to_slide[r_id] = tgt.split("slides/")[-1]

    # Extract active rIds from presentation.xml
    body = pres_xml.read_text(encoding="utf-8")
    active_rids = set(SLIDE_ID_RID_PATTERN.findall(body))

    return {rid_to_slide[k] for k in active_rids if k in rid_to_slide}


# ---------------------------------------------------------------------------
# Orphan slide purging
# ---------------------------------------------------------------------------


def purge_orphan_slides(pkg_root: Path) -> list[str]:
    """Delete slide files not referenced in the presentation manifest."""
    sld_dir = pkg_root / "ppt" / "slides"
    sld_rels_dir = sld_dir / "_rels"
    pres_rels = pkg_root / "ppt" / "_rels" / "presentation.xml.rels"

    if not sld_dir.exists():
        return []

    active = resolve_active_slides(pkg_root)
    deleted: list[str] = []

    for fp in sld_dir.glob("slide*.xml"):
        if fp.name in active:
            continue
        deleted.append(str(fp.relative_to(pkg_root)))
        fp.unlink()

        companion = sld_rels_dir / ("%s.rels" % fp.name)
        if companion.exists():
            companion.unlink()
            deleted.append(str(companion.relative_to(pkg_root)))

    # Clean up presentation.xml.rels entries for removed slides
    if deleted and pres_rels.exists():
        dom = defusedxml.minidom.parse(str(pres_rels))
        modified = False

        for node in list(dom.getElementsByTagName(RELATIONSHIP_TAG)):
            tgt = node.getAttribute("Target")
            if tgt.startswith("slides/"):
                sname = tgt.split("slides/")[-1]
                if sname not in active and node.parentNode:
                    node.parentNode.removeChild(node)
                    modified = True

        if modified:
            with open(pres_rels, "wb") as fh:
                fh.write(dom.toxml(encoding="utf-8"))

    return deleted


# ---------------------------------------------------------------------------
# Trash folder removal
# ---------------------------------------------------------------------------


def purge_trash_folder(pkg_root: Path) -> list[str]:
    """Remove the [trash] staging directory if present."""
    trash = pkg_root / TRASH_DIR_NAME
    removed: list[str] = []

    if trash.exists() and trash.is_dir():
        for item in trash.iterdir():
            if item.is_file():
                removed.append(str(item.relative_to(pkg_root)))
                item.unlink()
        trash.rmdir()

    return removed


# ---------------------------------------------------------------------------
# Reference collection
# ---------------------------------------------------------------------------


def collect_slide_targets(pkg_root: Path) -> set[Path]:
    """Gather all file paths referenced from slide relationship files."""
    refs: set[Path] = set()
    rels_home = pkg_root / "ppt" / "slides" / "_rels"

    if not rels_home.exists():
        return refs

    pkg_resolved = pkg_root.resolve()

    for rfile in rels_home.glob("*.rels"):
        dom = defusedxml.minidom.parse(str(rfile))
        for node in dom.getElementsByTagName(RELATIONSHIP_TAG):
            tgt = node.getAttribute("Target")
            if not tgt:
                continue
            resolved = (rfile.parent.parent / tgt).resolve()
            try:
                refs.add(resolved.relative_to(pkg_resolved))
            except ValueError:
                pass

    return refs


def collect_all_refs(pkg_root: Path) -> set[Path]:
    """Traverse every .rels file to build a complete set of referenced paths."""
    refs: set[Path] = set()
    pkg_resolved = pkg_root.resolve()

    for rfile in pkg_root.rglob("*.rels"):
        dom = defusedxml.minidom.parse(str(rfile))
        for node in dom.getElementsByTagName(RELATIONSHIP_TAG):
            tgt = node.getAttribute("Target")
            if not tgt:
                continue
            resolved = (rfile.parent.parent / tgt).resolve()
            try:
                refs.add(resolved.relative_to(pkg_resolved))
            except ValueError:
                pass

    return refs


# ---------------------------------------------------------------------------
# Stale relationship purging
# ---------------------------------------------------------------------------


def purge_stale_rels(pkg_root: Path) -> list[str]:
    """Remove .rels files whose parent resource no longer exists or is unreferenced."""
    removed: list[str] = []
    slide_refs = collect_slide_targets(pkg_root)
    pkg_resolved = pkg_root.resolve()

    for dname in CHART_LIKE_DIRS:
        rels_home = pkg_root / "ppt" / dname / "_rels"
        if not rels_home.exists():
            continue

        for rfile in rels_home.glob("*.rels"):
            parent_res = rels_home.parent / rfile.name.replace(".rels", "")
            try:
                parent_rel = parent_res.resolve().relative_to(pkg_resolved)
            except ValueError:
                continue

            if not parent_res.exists() or parent_rel not in slide_refs:
                rfile.unlink()
                removed.append(str(rfile.relative_to(pkg_root)))

    return removed


# ---------------------------------------------------------------------------
# Unreferenced asset purging
# ---------------------------------------------------------------------------


def purge_unreferenced_assets(pkg_root: Path, all_refs: set[Path]) -> list[str]:
    """Delete files in resource directories that nothing points to."""
    removed: list[str] = []

    # Standard resource categories
    for dname in RESOURCE_CATEGORIES:
        folder = pkg_root / "ppt" / dname
        if not folder.exists():
            continue
        for fp in folder.glob("*"):
            if not fp.is_file():
                continue
            rp = fp.relative_to(pkg_root)
            if rp not in all_refs:
                fp.unlink()
                removed.append(str(rp))

    # Theme files
    theme_folder = pkg_root / "ppt" / "theme"
    if theme_folder.exists():
        for fp in theme_folder.glob("theme*.xml"):
            rp = fp.relative_to(pkg_root)
            if rp not in all_refs:
                fp.unlink()
                removed.append(str(rp))
                companion = theme_folder / "_rels" / ("%s.rels" % fp.name)
                if companion.exists():
                    companion.unlink()
                    removed.append(str(companion.relative_to(pkg_root)))

    # Notes slides
    notes_folder = pkg_root / "ppt" / "notesSlides"
    if notes_folder.exists():
        for fp in notes_folder.glob("*.xml"):
            if not fp.is_file():
                continue
            rp = fp.relative_to(pkg_root)
            if rp not in all_refs:
                fp.unlink()
                removed.append(str(rp))

        notes_rels = notes_folder / "_rels"
        if notes_rels.exists():
            for fp in notes_rels.glob("*.rels"):
                parent_xml = notes_folder / fp.name.replace(".rels", "")
                if not parent_xml.exists():
                    fp.unlink()
                    removed.append(str(fp.relative_to(pkg_root)))

    return removed


# ---------------------------------------------------------------------------
# ContentType synchronization
# ---------------------------------------------------------------------------


def sync_content_types(pkg_root: Path, removed: list[str]) -> None:
    """Strip ContentType overrides for files that were deleted."""
    ct = pkg_root / "[Content_Types].xml"
    if not ct.exists():
        return

    dom = defusedxml.minidom.parse(str(ct))
    touched = False

    for ovr in list(dom.getElementsByTagName(OVERRIDE_TAG)):
        pn = ovr.getAttribute("PartName").lstrip("/")
        if pn in removed and ovr.parentNode:
            ovr.parentNode.removeChild(ovr)
            touched = True

    if touched:
        with open(ct, "wb") as fh:
            fh.write(dom.toxml(encoding="utf-8"))


# ---------------------------------------------------------------------------
# Main cleanup pipeline
# ---------------------------------------------------------------------------


def execute_cleanup(pkg_root: Path) -> list[str]:
    """Run the full cleanup pipeline and return a list of removed paths."""
    report = CleanupReport()

    report.extend(purge_orphan_slides(pkg_root))
    report.extend(purge_trash_folder(pkg_root))

    # Iteratively remove stale refs and unreferenced assets until stable
    while True:
        batch_rels = purge_stale_rels(pkg_root)
        refs = collect_all_refs(pkg_root)
        batch_assets = purge_unreferenced_assets(pkg_root, refs)

        combined = batch_rels + batch_assets
        if not combined:
            break
        report.extend(combined)

    if report.count > 0:
        sync_content_types(pkg_root, report.removed_paths)

    return report.removed_paths


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------


def main() -> None:
    """Parse arguments and execute the cleanup pipeline."""
    if len(sys.argv) != 2:
        print("Usage: python clean.py <unpacked_dir>", file=sys.stderr)
        print("Example: python clean.py unpacked/", file=sys.stderr)
        sys.exit(1)

    root = Path(sys.argv[1])

    if not root.exists():
        print("Error: %s not found" % root, file=sys.stderr)
        sys.exit(1)

    result = execute_cleanup(root)

    if result:
        print("Removed %d unreferenced files:" % len(result))
        for entry in result:
            print("  %s" % entry)
    else:
        print("No unreferenced files found")


if __name__ == "__main__":
    main()
