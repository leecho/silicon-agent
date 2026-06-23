"""
Schema and structural validator for PowerPoint (.pptx) presentation XML.
"""

import re

from .base import BaseSchemaValidator


class PPTXSchemaValidator(BaseSchemaValidator):

    _PML_NS = "http://schemas.openxmlformats.org/presentationml/2006/main"

    ELEMENT_RELATIONSHIP_TYPES = {
        "sldid": "slide",
        "sldmasterid": "slidemaster",
        "notesmasterid": "notesmaster",
        "sldlayoutid": "slidelayout",
        "themeid": "theme",
        "tablestyleid": "tablestyles",
    }

    # ------------------------------------------------------------------
    # Top-level orchestration
    # ------------------------------------------------------------------

    def validate(self):
        if not self.validate_xml():
            return False

        ok = True
        for check in (
            self.validate_namespaces,
            self.validate_unique_ids,
            self._check_uuid_format,
            self.validate_file_references,
            self._check_layout_refs,
            self.validate_content_types,
            self.validate_against_xsd,
            self._check_notes_refs,
            self.validate_all_relationship_ids,
            self._check_duplicate_layouts,
        ):
            if not check():
                ok = False

        return ok

    # ------------------------------------------------------------------
    # UUID format audit
    # ------------------------------------------------------------------

    _UUID_RE = re.compile(
        r"^[\{\(]?[0-9A-Fa-f]{8}-?[0-9A-Fa-f]{4}-?[0-9A-Fa-f]{4}-?[0-9A-Fa-f]{4}-?[0-9A-Fa-f]{12}[\}\)]?$"
    )

    def _check_uuid_format(self):
        import lxml.etree

        problems = []
        for xf in self.xml_files:
            try:
                root = lxml.etree.parse(str(xf)).getroot()
                for el in root.iter():
                    for attr, val in el.attrib.items():
                        aname = attr.split("}")[-1].lower()
                        if aname == "id" or aname.endswith("id"):
                            if self._resembles_uuid(val) and not self._UUID_RE.match(val):
                                problems.append(
                                    "  %s: Line %d: ID '%s' appears to be a UUID but contains invalid hex characters"
                                    % (xf.relative_to(self.unpacked_dir), el.sourceline, val)
                                )
            except (lxml.etree.XMLSyntaxError, Exception) as exc:
                problems.append("  %s: Error: %s" % (xf.relative_to(self.unpacked_dir), exc))

        if problems:
            print("FAILED - Found %d UUID ID validation errors:" % len(problems))
            for p in problems:
                print(p)
            return False
        if self.verbose:
            print("PASSED - All UUID-like IDs contain valid hex values")
        return True

    # Alias
    validate_uuid_ids = _check_uuid_format

    @staticmethod
    def _resembles_uuid(val: str) -> bool:
        stripped = val.strip("{}()").replace("-", "")
        return len(stripped) == 32 and stripped.isalnum()

    # Alias
    _looks_like_uuid = _resembles_uuid

    # ------------------------------------------------------------------
    # Slide-layout reference validation
    # ------------------------------------------------------------------

    def _check_layout_refs(self):
        import lxml.etree

        problems = []
        masters = list(self.unpacked_dir.glob("ppt/slideMasters/*.xml"))

        if not masters:
            if self.verbose:
                print("PASSED - No slide masters found")
            return True

        for master_fp in masters:
            try:
                root = lxml.etree.parse(str(master_fp)).getroot()
                rels_fp = master_fp.parent / "_rels" / ("%s.rels" % master_fp.name)

                if not rels_fp.exists():
                    problems.append(
                        "  %s: Missing relationships file: %s"
                        % (master_fp.relative_to(self.unpacked_dir), rels_fp.relative_to(self.unpacked_dir))
                    )
                    continue

                rroot = lxml.etree.parse(str(rels_fp)).getroot()
                layout_rids = {
                    rel.get("Id")
                    for rel in rroot.findall("{%s}Relationship" % self.NS_PKG_RELS)
                    if "slideLayout" in rel.get("Type", "")
                }

                for sli in root.findall(".//{%s}sldLayoutId" % self._PML_NS):
                    r_id = sli.get("{%s}id" % self.NS_OFFICE_RELS)
                    lid = sli.get("id")
                    if r_id and r_id not in layout_rids:
                        problems.append(
                            "  %s: Line %d: sldLayoutId with id='%s' references r:id='%s' which is not found in slide layout relationships"
                            % (master_fp.relative_to(self.unpacked_dir), sli.sourceline, lid, r_id)
                        )
            except (lxml.etree.XMLSyntaxError, Exception) as exc:
                problems.append("  %s: Error: %s" % (master_fp.relative_to(self.unpacked_dir), exc))

        if problems:
            print("FAILED - Found %d slide layout ID validation errors:" % len(problems))
            for p in problems:
                print(p)
            print("Remove invalid references or add missing slide layouts to the relationships file.")
            return False
        if self.verbose:
            print("PASSED - All slide layout IDs reference valid slide layouts")
        return True

    # Alias
    validate_slide_layout_ids = _check_layout_refs

    # ------------------------------------------------------------------
    # Notes-slide deduplication check
    # ------------------------------------------------------------------

    def _check_notes_refs(self):
        import lxml.etree

        problems = []
        notes_map = {}

        rels_files = list(self.unpacked_dir.glob("ppt/slides/_rels/*.xml.rels"))
        if not rels_files:
            if self.verbose:
                print("PASSED - No slide relationship files found")
            return True

        for rf in rels_files:
            try:
                root = lxml.etree.parse(str(rf)).getroot()
                for rel in root.findall("{%s}Relationship" % self.NS_PKG_RELS):
                    if "notesSlide" not in rel.get("Type", ""):
                        continue
                    tgt = rel.get("Target", "").replace("../", "")
                    if not tgt:
                        continue
                    slide_label = rf.stem.replace(".xml", "")
                    notes_map.setdefault(tgt, []).append((slide_label, rf))
            except (lxml.etree.XMLSyntaxError, Exception) as exc:
                problems.append("  %s: Error: %s" % (rf.relative_to(self.unpacked_dir), exc))

        for tgt, refs in notes_map.items():
            if len(refs) > 1:
                names = [r[0] for r in refs]
                problems.append("  Notes slide '%s' is referenced by multiple slides: %s" % (tgt, ", ".join(names)))
                for name, rf in refs:
                    problems.append("    - %s" % rf.relative_to(self.unpacked_dir))

        if problems:
            top_count = len([p for p in problems if not p.startswith("    ")])
            print("FAILED - Found %d notes slide reference validation errors:" % top_count)
            for p in problems:
                print(p)
            print("Each slide may optionally have its own slide file.")
            return False
        if self.verbose:
            print("PASSED - All notes slide references are unique")
        return True

    # Alias
    validate_notes_slide_references = _check_notes_refs

    # ------------------------------------------------------------------
    # Duplicate layout detection
    # ------------------------------------------------------------------

    def _check_duplicate_layouts(self):
        import lxml.etree

        problems = []
        for rf in self.unpacked_dir.glob("ppt/slides/_rels/*.xml.rels"):
            try:
                root = lxml.etree.parse(str(rf)).getroot()
                layouts = [
                    rel for rel in root.findall("{%s}Relationship" % self.NS_PKG_RELS)
                    if "slideLayout" in rel.get("Type", "")
                ]
                if len(layouts) > 1:
                    problems.append(
                        "  %s: has %d slideLayout references"
                        % (rf.relative_to(self.unpacked_dir), len(layouts))
                    )
            except Exception as exc:
                problems.append("  %s: Error: %s" % (rf.relative_to(self.unpacked_dir), exc))

        if problems:
            print("FAILED - Found slides with duplicate slideLayout references:")
            for p in problems:
                print(p)
            return False
        if self.verbose:
            print("PASSED - All slides have exactly one slideLayout reference")
        return True

    # Alias
    validate_no_duplicate_slide_layouts = _check_duplicate_layouts


if __name__ == "__main__":
    raise RuntimeError("This module should not be run directly.")
