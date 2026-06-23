"""
CLI tool for checking Office document XML against XSD schemas and tracked-change rules.

Invocation:
    python validate.py <target> [--original <original_file>] [--auto-repair] [--author NAME]

The first positional argument may be:
- A directory containing extracted Office XML files
- A packaged Office file (.docx/.pptx/.xlsx) that will be auto-extracted to a temp location

Auto-repair capabilities:
- Clamps paraId/durableId values that exceed OOXML upper bounds
- Inserts missing xml:space="preserve" on w:t elements containing boundary whitespace
"""

import argparse
import sys
import tempfile
import zipfile
from pathlib import Path

from validators import DOCXSchemaValidator, PPTXSchemaValidator, RedliningValidator

_OFFICE_SUFFIXES = [".docx", ".pptx", ".xlsx"]


def main():
    ap = argparse.ArgumentParser(description="Validate Office document XML files")
    ap.add_argument(
        "path",
        help="Path to extracted directory or packaged Office file (.docx/.pptx/.xlsx)",
    )
    ap.add_argument(
        "--original",
        required=False,
        default=None,
        help="Path to original file (.docx/.pptx/.xlsx). If omitted, all XSD errors are reported and redlining validation is skipped.",
    )
    ap.add_argument(
        "-v", "--verbose",
        action="store_true",
        help="Enable verbose output",
    )
    ap.add_argument(
        "--auto-repair",
        action="store_true",
        help="Automatically repair common issues (hex IDs, whitespace preservation)",
    )
    ap.add_argument(
        "--author",
        default="Claude",
        help="Author name for redlining validation (default: Claude)",
    )
    opts = ap.parse_args()

    target = Path(opts.path)
    assert target.exists(), "Error: %s does not exist" % target

    ref_file = None
    if opts.original:
        ref_file = Path(opts.original)
        assert ref_file.is_file(), "Error: %s is not a file" % ref_file
        assert ref_file.suffix.lower() in _OFFICE_SUFFIXES, (
            "Error: %s must be a .docx, .pptx, or .xlsx file" % ref_file
        )

    ext = (ref_file or target).suffix.lower()
    assert ext in _OFFICE_SUFFIXES, (
        "Error: Cannot determine file type from %s. Use --original or provide a .docx/.pptx/.xlsx file." % target
    )

    # If a packed file is given, extract it first
    if target.is_file() and target.suffix.lower() in _OFFICE_SUFFIXES:
        work_dir = tempfile.mkdtemp()
        with zipfile.ZipFile(target, "r") as zf:
            zf.extractall(work_dir)
        pkg = Path(work_dir)
    else:
        assert target.is_dir(), "Error: %s is not a directory or Office file" % target
        pkg = target

    # Build validator list based on file type
    match ext:
        case ".docx":
            checkers = [
                DOCXSchemaValidator(pkg, ref_file, verbose=opts.verbose),
            ]
            if ref_file:
                checkers.append(
                    RedliningValidator(pkg, ref_file, verbose=opts.verbose, author=opts.author)
                )
        case ".pptx":
            checkers = [
                PPTXSchemaValidator(pkg, ref_file, verbose=opts.verbose),
            ]
        case _:
            print("Error: Validation not supported for file type %s" % ext)
            sys.exit(1)

    if opts.auto_repair:
        n_fixed = sum(c.repair() for c in checkers)
        if n_fixed:
            print("Auto-repaired %d issue(s)" % n_fixed)

    all_ok = all(c.validate() for c in checkers)

    if all_ok:
        print("All validations PASSED!")

    sys.exit(0 if all_ok else 1)


if __name__ == "__main__":
    main()
