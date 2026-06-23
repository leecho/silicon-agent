"""
Schema and structural validator for Word (.docx) document XML.
"""

import random
import re
import tempfile
import zipfile

import defusedxml.minidom
import lxml.etree

from .base import BaseSchemaValidator


class DOCXSchemaValidator(BaseSchemaValidator):

    _WP_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    _W14_NS = "http://schemas.microsoft.com/office/word/2010/wordml"
    _W16CID_NS = "http://schemas.microsoft.com/office/word/2016/wordml/cid"

    ELEMENT_RELATIONSHIP_TYPES = {}

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
            self.validate_file_references,
            self.validate_content_types,
            self.validate_against_xsd,
            self._check_whitespace,
            self._check_deletions,
            self._check_insertions,
            self.validate_all_relationship_ids,
            self._check_id_bounds,
            self._check_comment_markers,
        ):
            if not check():
                ok = False

        self._report_paragraph_delta()
        return ok

    # ------------------------------------------------------------------
    # Whitespace preservation audit
    # ------------------------------------------------------------------

    def _check_whitespace(self):
        problems = []
        for xf in self.xml_files:
            if xf.name != "document.xml":
                continue
            try:
                root = lxml.etree.parse(str(xf)).getroot()
                xml_space_key = "{%s}space" % self.XML_NAMESPACE
                for t_el in root.iter("{%s}t" % self._WP_NS):
                    if not t_el.text:
                        continue
                    txt = t_el.text
                    if re.search(r"^[ \t\n\r]", txt) or re.search(r"[ \t\n\r]$", txt):
                        if xml_space_key not in t_el.attrib or t_el.attrib[xml_space_key] != "preserve":
                            preview = repr(txt)[:50] + "..." if len(repr(txt)) > 50 else repr(txt)
                            problems.append(
                                "  %s: Line %d: w:t element with whitespace missing xml:space='preserve': %s"
                                % (xf.relative_to(self.unpacked_dir), t_el.sourceline, preview)
                            )
            except (lxml.etree.XMLSyntaxError, Exception) as exc:
                problems.append("  %s: Error: %s" % (xf.relative_to(self.unpacked_dir), exc))

        if problems:
            print("FAILED - Found %d whitespace preservation violations:" % len(problems))
            for p in problems:
                print(p)
            return False
        if self.verbose:
            print("PASSED - All whitespace is properly preserved")
        return True

    # Alias for interface consistency
    validate_whitespace_preservation = _check_whitespace

    # ------------------------------------------------------------------
    # Deletion structure audit
    # ------------------------------------------------------------------

    def _check_deletions(self):
        problems = []
        ns_map = {"w": self._WP_NS}

        for xf in self.xml_files:
            if xf.name != "document.xml":
                continue
            try:
                root = lxml.etree.parse(str(xf)).getroot()

                for t_el in root.xpath(".//w:del//w:t", namespaces=ns_map):
                    if t_el.text:
                        preview = repr(t_el.text)[:50] + "..." if len(repr(t_el.text)) > 50 else repr(t_el.text)
                        problems.append(
                            "  %s: Line %d: <w:t> found within <w:del>: %s"
                            % (xf.relative_to(self.unpacked_dir), t_el.sourceline, preview)
                        )

                for ie in root.xpath(".//w:del//w:instrText", namespaces=ns_map):
                    preview = repr(ie.text or "")[:50] + "..." if len(repr(ie.text or "")) > 50 else repr(ie.text or "")
                    problems.append(
                        "  %s: Line %d: <w:instrText> found within <w:del> (use <w:delInstrText>): %s"
                        % (xf.relative_to(self.unpacked_dir), ie.sourceline, preview)
                    )
            except (lxml.etree.XMLSyntaxError, Exception) as exc:
                problems.append("  %s: Error: %s" % (xf.relative_to(self.unpacked_dir), exc))

        if problems:
            print("FAILED - Found %d deletion validation violations:" % len(problems))
            for p in problems:
                print(p)
            return False
        if self.verbose:
            print("PASSED - No w:t elements found within w:del elements")
        return True

    # Alias
    validate_deletions = _check_deletions

    # ------------------------------------------------------------------
    # Insertion structure audit
    # ------------------------------------------------------------------

    def _check_insertions(self):
        problems = []
        ns_map = {"w": self._WP_NS}

        for xf in self.xml_files:
            if xf.name != "document.xml":
                continue
            try:
                root = lxml.etree.parse(str(xf)).getroot()
                bad = root.xpath(".//w:ins//w:delText[not(ancestor::w:del)]", namespaces=ns_map)
                for el in bad:
                    preview = repr(el.text or "")[:50] + "..." if len(repr(el.text or "")) > 50 else repr(el.text or "")
                    problems.append(
                        "  %s: Line %d: <w:delText> within <w:ins>: %s"
                        % (xf.relative_to(self.unpacked_dir), el.sourceline, preview)
                    )
            except (lxml.etree.XMLSyntaxError, Exception) as exc:
                problems.append("  %s: Error: %s" % (xf.relative_to(self.unpacked_dir), exc))

        if problems:
            print("FAILED - Found %d insertion validation violations:" % len(problems))
            for p in problems:
                print(p)
            return False
        if self.verbose:
            print("PASSED - No w:delText elements within w:ins elements")
        return True

    # Alias
    validate_insertions = _check_insertions

    # ------------------------------------------------------------------
    # Paragraph count comparison
    # ------------------------------------------------------------------

    def _count_paragraphs_unpacked(self) -> int:
        for xf in self.xml_files:
            if xf.name != "document.xml":
                continue
            try:
                root = lxml.etree.parse(str(xf)).getroot()
                return len(root.findall(".//{%s}p" % self._WP_NS))
            except Exception as exc:
                print("Error counting paragraphs in unpacked document: %s" % exc)
        return 0

    # Alias
    count_paragraphs_in_unpacked = _count_paragraphs_unpacked

    def _count_paragraphs_original(self) -> int:
        if self.original_file is None:
            return 0
        try:
            with tempfile.TemporaryDirectory() as td:
                with zipfile.ZipFile(self.original_file, "r") as zf:
                    zf.extractall(td)
                root = lxml.etree.parse(td + "/word/document.xml").getroot()
                return len(root.findall(".//{%s}p" % self._WP_NS))
        except Exception as exc:
            print("Error counting paragraphs in original document: %s" % exc)
            return 0

    # Alias
    count_paragraphs_in_original = _count_paragraphs_original

    def _report_paragraph_delta(self):
        orig = self._count_paragraphs_original()
        curr = self._count_paragraphs_unpacked()
        delta = curr - orig
        sign = "+%d" % delta if delta > 0 else str(delta)
        print("\nParagraphs: %d \u2192 %d (%s)" % (orig, curr, sign))

    # Alias
    compare_paragraph_counts = _report_paragraph_delta

    # ------------------------------------------------------------------
    # ID constraint validation
    # ------------------------------------------------------------------

    @staticmethod
    def _to_int(val: str, base: int = 16) -> int:
        return int(val, base)

    # Alias
    _parse_id_value = _to_int

    def _check_id_bounds(self):
        problems = []
        pid_attr = "{%s}paraId" % self._W14_NS
        did_attr = "{%s}durableId" % self._W16CID_NS

        for xf in self.xml_files:
            try:
                for el in lxml.etree.parse(str(xf)).iter():
                    v = el.get(pid_attr)
                    if v and self._to_int(v, 16) >= 0x80000000:
                        problems.append("  %s:%s: paraId=%s >= 0x80000000" % (xf.name, el.sourceline, v))

                    v = el.get(did_attr)
                    if v:
                        if xf.name == "numbering.xml":
                            try:
                                if self._to_int(v, 10) >= 0x7FFFFFFF:
                                    problems.append("  %s:%s: durableId=%s >= 0x7FFFFFFF" % (xf.name, el.sourceline, v))
                            except ValueError:
                                problems.append("  %s:%s: durableId=%s must be decimal in numbering.xml" % (xf.name, el.sourceline, v))
                        elif self._to_int(v, 16) >= 0x7FFFFFFF:
                            problems.append("  %s:%s: durableId=%s >= 0x7FFFFFFF" % (xf.name, el.sourceline, v))
            except Exception:
                pass

        if problems:
            print("FAILED - %d ID constraint violations:" % len(problems))
            for p in problems:
                print(p)
        elif self.verbose:
            print("PASSED - All paraId/durableId values within constraints")
        return not bool(problems)

    # Alias
    validate_id_constraints = _check_id_bounds

    # ------------------------------------------------------------------
    # Comment marker consistency
    # ------------------------------------------------------------------

    def _check_comment_markers(self):
        problems = []
        doc_xf = None
        comments_xf = None

        for xf in self.xml_files:
            if xf.name == "document.xml" and "word" in str(xf):
                doc_xf = xf
            elif xf.name == "comments.xml":
                comments_xf = xf

        if doc_xf is None:
            if self.verbose:
                print("PASSED - No document.xml found (skipping comment validation)")
            return True

        try:
            root = lxml.etree.parse(str(doc_xf)).getroot()
            ns = {"w": self._WP_NS}
            w_id = "{%s}id" % self._WP_NS

            starts = {el.get(w_id) for el in root.xpath(".//w:commentRangeStart", namespaces=ns)}
            ends = {el.get(w_id) for el in root.xpath(".//w:commentRangeEnd", namespaces=ns)}
            refs = {el.get(w_id) for el in root.xpath(".//w:commentReference", namespaces=ns)}

            _sort_key = lambda x: int(x) if x and x.isdigit() else 0

            for cid in sorted(ends - starts, key=_sort_key):
                problems.append('  document.xml: commentRangeEnd id="%s" has no matching commentRangeStart' % cid)
            for cid in sorted(starts - ends, key=_sort_key):
                problems.append('  document.xml: commentRangeStart id="%s" has no matching commentRangeEnd' % cid)

            if comments_xf and comments_xf.exists():
                croot = lxml.etree.parse(str(comments_xf)).getroot()
                defined = {el.get(w_id) for el in croot.xpath(".//w:comment", namespaces=ns)}
                dangling = (starts | ends | refs) - defined
                for cid in sorted(dangling, key=_sort_key):
                    if cid:
                        problems.append('  document.xml: marker id="%s" references non-existent comment' % cid)

        except (lxml.etree.XMLSyntaxError, Exception) as exc:
            problems.append("  Error parsing XML: %s" % exc)

        if problems:
            print("FAILED - %d comment marker violations:" % len(problems))
            for p in problems:
                print(p)
            return False
        if self.verbose:
            print("PASSED - All comment markers properly paired")
        return True

    # Alias
    validate_comment_markers = _check_comment_markers

    # ------------------------------------------------------------------
    # Repair: durableId overflow
    # ------------------------------------------------------------------

    def repair(self) -> int:
        n = super().repair()
        n += self._fix_durable_ids()
        return n

    def _fix_durable_ids(self) -> int:
        n_fixed = 0
        for xf in self.xml_files:
            try:
                raw = xf.read_text(encoding="utf-8")
                dom = defusedxml.minidom.parseString(raw)
                touched = False

                for el in dom.getElementsByTagName("*"):
                    if not el.hasAttribute("w16cid:durableId"):
                        continue
                    old_val = el.getAttribute("w16cid:durableId")
                    needs_fix = False

                    if xf.name == "numbering.xml":
                        try:
                            needs_fix = self._to_int(old_val, 10) >= 0x7FFFFFFF
                        except ValueError:
                            needs_fix = True
                    else:
                        try:
                            needs_fix = self._to_int(old_val, 16) >= 0x7FFFFFFF
                        except ValueError:
                            needs_fix = True

                    if needs_fix:
                        fresh = random.randint(1, 0x7FFFFFFE)
                        new_val = str(fresh) if xf.name == "numbering.xml" else "%08X" % fresh
                        el.setAttribute("w16cid:durableId", new_val)
                        print("  Repaired: %s: durableId %s \u2192 %s" % (xf.name, old_val, new_val))
                        n_fixed += 1
                        touched = True

                if touched:
                    xf.write_bytes(dom.toxml(encoding="UTF-8"))
            except Exception:
                pass
        return n_fixed

    # Alias
    repair_durableId = _fix_durable_ids


if __name__ == "__main__":
    raise RuntimeError("This module should not be run directly.")
