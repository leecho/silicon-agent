"""Extract Office archives (DOCX, PPTX, XLSX) for manual editing.

Decompresses the ZIP container, reformats XML for readability, and optionally:
- Consolidates adjacent runs sharing identical formatting (DOCX only)
- Collapses sequential tracked changes by the same author (DOCX only)

Invocation:
    python unpack.py <office_file> <dest_dir> [options]

Samples:
    python unpack.py document.docx unpacked/
    python unpack.py presentation.pptx unpacked/
    python unpack.py document.docx unpacked/ --merge-runs false
"""

import argparse
import sys
import zipfile
from pathlib import Path

import defusedxml.minidom

from helpers.merge_runs import merge_runs as consolidate_runs
from helpers.simplify_redlines import simplify_redlines as collapse_redlines

_CURLY_QUOTE_MAP = {
    "\u201c": "&#x201C;",
    "\u201d": "&#x201D;",
    "\u2018": "&#x2018;",
    "\u2019": "&#x2019;",
}

_VALID_SUFFIXES = {".docx", ".pptx", ".xlsx"}


def _reformat_xml(fp: Path) -> None:
    """Pretty-print a single XML file in place."""
    try:
        raw = fp.read_text(encoding="utf-8")
        dom = defusedxml.minidom.parseString(raw)
        fp.write_bytes(dom.toprettyxml(indent="  ", encoding="utf-8"))
    except Exception:
        pass


def _normalize_quotes(fp: Path) -> None:
    """Replace Unicode curly quotes with their XML entity equivalents."""
    try:
        txt = fp.read_text(encoding="utf-8")
        for ch, entity in _CURLY_QUOTE_MAP.items():
            txt = txt.replace(ch, entity)
        fp.write_text(txt, encoding="utf-8")
    except Exception:
        pass


def unpack(
    input_file: str,
    output_directory: str,
    merge_runs: bool = True,
    simplify_redlines: bool = True,
) -> tuple[None, str]:
    """Primary extraction entry point. Returns (None, status_message)."""
    src = Path(input_file)
    dest = Path(output_directory)
    ext = src.suffix.lower()

    if not src.exists():
        return None, "Error: %s does not exist" % input_file

    if ext not in _VALID_SUFFIXES:
        return None, "Error: %s must be a .docx, .pptx, or .xlsx file" % input_file

    try:
        dest.mkdir(parents=True, exist_ok=True)

        with zipfile.ZipFile(src, "r") as archive:
            archive.extractall(dest)

        xml_inventory = list(dest.rglob("*.xml")) + list(dest.rglob("*.rels"))
        for xf in xml_inventory:
            _reformat_xml(xf)

        status = "Unpacked %s (%d XML files)" % (input_file, len(xml_inventory))

        if ext == ".docx":
            if simplify_redlines:
                n_simplified, _ = collapse_redlines(str(dest))
                status += ", simplified %d tracked changes" % n_simplified

            if merge_runs:
                n_merged, _ = consolidate_runs(str(dest))
                status += ", merged %d runs" % n_merged

        for xf in xml_inventory:
            _normalize_quotes(xf)

        return None, status

    except zipfile.BadZipFile:
        return None, "Error: %s is not a valid Office file" % input_file
    except Exception as exc:
        return None, "Error unpacking: %s" % exc


if __name__ == "__main__":
    ap = argparse.ArgumentParser(
        description="Unpack an Office file (DOCX, PPTX, XLSX) for editing"
    )
    ap.add_argument("input_file", help="Office file to unpack")
    ap.add_argument("output_directory", help="Output directory")
    ap.add_argument(
        "--merge-runs",
        type=lambda v: v.lower() == "true",
        default=True,
        metavar="true|false",
        help="Merge adjacent runs with identical formatting (DOCX only, default: true)",
    )
    ap.add_argument(
        "--simplify-redlines",
        type=lambda v: v.lower() == "true",
        default=True,
        metavar="true|false",
        help="Merge adjacent tracked changes from same author (DOCX only, default: true)",
    )
    parsed = ap.parse_args()

    _, status = unpack(
        parsed.input_file,
        parsed.output_directory,
        merge_runs=parsed.merge_runs,
        simplify_redlines=parsed.simplify_redlines,
    )
    print(status)

    if "Error" in status:
        sys.exit(1)
