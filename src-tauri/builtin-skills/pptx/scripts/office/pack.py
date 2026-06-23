"""Reassemble a directory into a DOCX, PPTX, or XLSX archive.

Runs integrity checks with automatic repair, strips XML formatting whitespace,
then produces the final Office file.

Invocation:
    python pack.py <src_dir> <dest_file> [--original <file>] [--validate true|false]

Samples:
    python pack.py unpacked/ output.docx --original input.docx
    python pack.py unpacked/ output.pptx --validate false
"""

import argparse
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path

import defusedxml.minidom

from validators import DOCXSchemaValidator, PPTXSchemaValidator, RedliningValidator

SUPPORTED_EXTENSIONS = {".docx", ".pptx", ".xlsx"}


def _strip_xml_whitespace(fp: Path) -> None:
    """Remove pretty-print whitespace and comments from an XML file."""
    try:
        with open(fp, encoding="utf-8") as fh:
            dom = defusedxml.minidom.parse(fh)

        for el in dom.getElementsByTagName("*"):
            if el.tagName.endswith(":t"):
                continue
            for ch in list(el.childNodes):
                is_blank_text = (
                    ch.nodeType == ch.TEXT_NODE
                    and ch.nodeValue
                    and not ch.nodeValue.strip()
                )
                if is_blank_text or ch.nodeType == ch.COMMENT_NODE:
                    el.removeChild(ch)

        fp.write_bytes(dom.toxml(encoding="UTF-8"))
    except Exception as exc:
        print("ERROR: Failed to parse %s: %s" % (fp.name, exc), file=sys.stderr)
        raise


def _execute_validators(
    src: Path, orig: Path, ext: str, author_fn=None
) -> tuple[bool, str | None]:
    """Instantiate and run the appropriate validator chain."""
    messages = []
    checkers = []

    if ext == ".docx":
        writer = "Claude"
        if author_fn:
            try:
                writer = author_fn(src, orig)
            except ValueError as ve:
                print(
                    "Warning: %s Using default author 'Claude'." % ve,
                    file=sys.stderr,
                )
        checkers = [
            DOCXSchemaValidator(src, orig),
            RedliningValidator(src, orig, author=writer),
        ]
    elif ext == ".pptx":
        checkers = [PPTXSchemaValidator(src, orig)]

    if not checkers:
        return True, None

    fix_count = sum(c.repair() for c in checkers)
    if fix_count:
        messages.append("Auto-repaired %d issue(s)" % fix_count)

    ok = all(c.validate() for c in checkers)

    if ok:
        messages.append("All validations PASSED!")

    return ok, "\n".join(messages) if messages else None


def pack(
    input_directory: str,
    output_file: str,
    original_file: str | None = None,
    validate: bool = True,
    infer_author_func=None,
) -> tuple[None, str]:
    """Main packing entry point. Returns (None, status_message)."""
    src = Path(input_directory)
    dest = Path(output_file)
    ext = dest.suffix.lower()

    if not src.is_dir():
        return None, "Error: %s is not a directory" % src

    if ext not in SUPPORTED_EXTENSIONS:
        return None, "Error: %s must be a .docx, .pptx, or .xlsx file" % output_file

    if validate and original_file:
        orig = Path(original_file)
        if orig.exists():
            ok, report = _execute_validators(src, orig, ext, infer_author_func)
            if report:
                print(report)
            if not ok:
                return None, "Error: Validation failed for %s" % src

    with tempfile.TemporaryDirectory() as staging:
        staging_content = Path(staging) / "content"
        shutil.copytree(src, staging_content)

        for glob in ("*.xml", "*.rels"):
            for xf in staging_content.rglob(glob):
                _strip_xml_whitespace(xf)

        dest.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED) as archive:
            for fp in staging_content.rglob("*"):
                if fp.is_file():
                    archive.write(fp, fp.relative_to(staging_content))

    return None, "Successfully packed %s to %s" % (src, output_file)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(
        description="Pack a directory into a DOCX, PPTX, or XLSX file"
    )
    ap.add_argument("input_directory", help="Unpacked Office document directory")
    ap.add_argument("output_file", help="Output Office file (.docx/.pptx/.xlsx)")
    ap.add_argument("--original", help="Original file for validation comparison")
    ap.add_argument(
        "--validate",
        type=lambda v: v.lower() == "true",
        default=True,
        metavar="true|false",
        help="Run validation with auto-repair (default: true)",
    )
    parsed = ap.parse_args()

    _, msg = pack(
        parsed.input_directory,
        parsed.output_file,
        original_file=parsed.original,
        validate=parsed.validate,
    )
    print(msg)

    if "Error" in msg:
        sys.exit(1)
