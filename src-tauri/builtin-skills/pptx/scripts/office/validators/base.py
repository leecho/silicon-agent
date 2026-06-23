"""
Foundation validator providing shared logic for all Office document checks.
"""

import re
from pathlib import Path

import defusedxml.minidom
import lxml.etree


class BaseSchemaValidator:

    SKIPPABLE_ERROR_PATTERNS = [
        "hyphenationZone",
        "purl.org/dc/terms",
    ]

    ID_UNIQUENESS_RULES = {
        "comment": ("id", "file"),
        "commentrangestart": ("id", "file"),
        "commentrangeend": ("id", "file"),
        "bookmarkstart": ("id", "file"),
        "bookmarkend": ("id", "file"),
        "sldid": ("id", "file"),
        "sldmasterid": ("id", "global"),
        "sldlayoutid": ("id", "global"),
        "cm": ("authorid", "file"),
        "sheet": ("sheetid", "file"),
        "definedname": ("id", "file"),
        "cxnsp": ("id", "file"),
        "sp": ("id", "file"),
        "pic": ("id", "file"),
        "grpsp": ("id", "file"),
    }

    ID_CONTAINER_EXCLUSIONS = {
        "sectionlst",
    }

    ELEMENT_RELATIONSHIP_TYPES = {}

    XSD_LOOKUP = {
        "word": "ISO-IEC29500-4_2016/wml.xsd",
        "ppt": "ISO-IEC29500-4_2016/pml.xsd",
        "xl": "ISO-IEC29500-4_2016/sml.xsd",
        "[Content_Types].xml": "ecma/fouth-edition/opc-contentTypes.xsd",
        "app.xml": "ISO-IEC29500-4_2016/shared-documentPropertiesExtended.xsd",
        "core.xml": "ecma/fouth-edition/opc-coreProperties.xsd",
        "custom.xml": "ISO-IEC29500-4_2016/shared-documentPropertiesCustom.xsd",
        ".rels": "ecma/fouth-edition/opc-relationships.xsd",
        "people.xml": "microsoft/wml-2012.xsd",
        "commentsIds.xml": "microsoft/wml-cid-2016.xsd",
        "commentsExtensible.xml": "microsoft/wml-cex-2018.xsd",
        "commentsExtended.xml": "microsoft/wml-2012.xsd",
        "chart": "ISO-IEC29500-4_2016/dml-chart.xsd",
        "theme": "ISO-IEC29500-4_2016/dml-main.xsd",
        "drawing": "ISO-IEC29500-4_2016/dml-main.xsd",
    }

    NS_MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
    NS_XML = "http://www.w3.org/XML/1998/namespace"
    NS_PKG_RELS = "http://schemas.openxmlformats.org/package/2006/relationships"
    NS_OFFICE_RELS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    NS_CONTENT_TYPES = "http://schemas.openxmlformats.org/package/2006/content-types"

    CONTENT_ROOT_DIRS = {"word", "ppt", "xl"}

    STANDARD_OOXML_NS = {
        "http://schemas.openxmlformats.org/officeDocument/2006/math",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        "http://schemas.openxmlformats.org/schemaLibrary/2006/main",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
        "http://schemas.openxmlformats.org/drawingml/2006/chart",
        "http://schemas.openxmlformats.org/drawingml/2006/chartDrawing",
        "http://schemas.openxmlformats.org/drawingml/2006/diagram",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
        "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing",
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        "http://schemas.openxmlformats.org/presentationml/2006/main",
        "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
        "http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes",
        "http://www.w3.org/XML/1998/namespace",
    }

    def __init__(self, unpacked_dir, original_file=None, verbose=False):
        self.unpacked_dir = Path(unpacked_dir).resolve()
        self.original_file = Path(original_file) if original_file else None
        self.verbose = verbose
        self.schemas_dir = Path(__file__).parent.parent / "schemas"
        self.xml_files = [
            fp
            for pat in ("*.xml", "*.rels")
            for fp in self.unpacked_dir.rglob(pat)
        ]
        if not self.xml_files:
            print("Warning: No XML files found in %s" % self.unpacked_dir)

    # ------------------------------------------------------------------
    # Abstract interface
    # ------------------------------------------------------------------

    def validate(self):
        raise NotImplementedError("Subclasses must implement the validate method")

    def repair(self) -> int:
        return self._fix_whitespace_preservation()

    # ------------------------------------------------------------------
    # Repair: whitespace preservation
    # ------------------------------------------------------------------

    def _fix_whitespace_preservation(self) -> int:
        n_fixed = 0
        for xf in self.xml_files:
            try:
                raw = xf.read_text(encoding="utf-8")
                dom = defusedxml.minidom.parseString(raw)
                touched = False
                for el in dom.getElementsByTagName("*"):
                    if not el.tagName.endswith(":t") or not el.firstChild:
                        continue
                    txt = el.firstChild.nodeValue
                    if txt and (txt[0] in (' ', '\t') or txt[-1] in (' ', '\t')):
                        if el.getAttribute("xml:space") != "preserve":
                            el.setAttribute("xml:space", "preserve")
                            preview = repr(txt[:30]) + "..." if len(txt) > 30 else repr(txt)
                            print("  Repaired: %s: Added xml:space='preserve' to %s: %s" % (xf.name, el.tagName, preview))
                            n_fixed += 1
                            touched = True
                if touched:
                    xf.write_bytes(dom.toxml(encoding="UTF-8"))
            except Exception:
                pass
        return n_fixed

    # Alias for backward compatibility
    repair_whitespace_preservation = _fix_whitespace_preservation

    # ------------------------------------------------------------------
    # Check: well-formed XML
    # ------------------------------------------------------------------

    def validate_xml(self):
        issues = []
        for xf in self.xml_files:
            try:
                lxml.etree.parse(str(xf))
            except lxml.etree.XMLSyntaxError as exc:
                issues.append(
                    "  %s: Line %d: %s"
                    % (xf.relative_to(self.unpacked_dir), exc.lineno, exc.msg)
                )
            except Exception as exc:
                issues.append(
                    "  %s: Unexpected error: %s"
                    % (xf.relative_to(self.unpacked_dir), exc)
                )

        if issues:
            print("FAILED - Found %d XML violations:" % len(issues))
            for ln in issues:
                print(ln)
            return False
        if self.verbose:
            print("PASSED - All XML files are well-formed")
        return True

    # ------------------------------------------------------------------
    # Check: namespace declarations
    # ------------------------------------------------------------------

    def validate_namespaces(self):
        issues = []
        for xf in self.xml_files:
            try:
                root = lxml.etree.parse(str(xf)).getroot()
                declared = set(root.nsmap.keys()) - {None}
                for attr_val in [v for k, v in root.attrib.items() if k.endswith("Ignorable")]:
                    missing = set(attr_val.split()) - declared
                    issues.extend(
                        "  %s: Namespace '%s' in Ignorable but not declared"
                        % (xf.relative_to(self.unpacked_dir), ns)
                        for ns in missing
                    )
            except lxml.etree.XMLSyntaxError:
                continue

        if issues:
            print("FAILED - %d namespace issues:" % len(issues))
            for ln in issues:
                print(ln)
            return False
        if self.verbose:
            print("PASSED - All namespace prefixes properly declared")
        return True

    # ------------------------------------------------------------------
    # Check: unique IDs
    # ------------------------------------------------------------------

    def validate_unique_ids(self):
        issues = []
        global_registry = {}

        for xf in self.xml_files:
            try:
                root = lxml.etree.parse(str(xf)).getroot()
                per_file = {}

                # Strip MC alternate content before scanning
                for mc in root.xpath(".//mc:AlternateContent", namespaces={"mc": self.NS_MC}):
                    mc.getparent().remove(mc)

                for el in root.iter():
                    tag_lower = el.tag.split("}")[-1].lower() if "}" in el.tag else el.tag.lower()

                    if tag_lower not in self.ID_UNIQUENESS_RULES:
                        continue

                    # Skip elements inside excluded containers
                    in_excluded = any(
                        anc.tag.split("}")[-1].lower() in self.ID_CONTAINER_EXCLUSIONS
                        for anc in el.iterancestors()
                    )
                    if in_excluded:
                        continue

                    attr_name, scope = self.ID_UNIQUENESS_RULES[tag_lower]
                    id_val = None
                    for ak, av in el.attrib.items():
                        ak_local = ak.split("}")[-1].lower() if "}" in ak else ak.lower()
                        if ak_local == attr_name:
                            id_val = av
                            break

                    if id_val is None:
                        continue

                    rel_path = xf.relative_to(self.unpacked_dir)

                    if scope == "global":
                        if id_val in global_registry:
                            pf, pl, pt = global_registry[id_val]
                            issues.append(
                                "  %s: Line %d: Global ID '%s' in <%s> already used in %s at line %d in <%s>"
                                % (rel_path, el.sourceline, id_val, tag_lower, pf, pl, pt)
                            )
                        else:
                            global_registry[id_val] = (rel_path, el.sourceline, tag_lower)
                    else:
                        key = (tag_lower, attr_name)
                        if key not in per_file:
                            per_file[key] = {}
                        if id_val in per_file[key]:
                            issues.append(
                                "  %s: Line %d: Duplicate %s='%s' in <%s> (first occurrence at line %d)"
                                % (rel_path, el.sourceline, attr_name, id_val, tag_lower, per_file[key][id_val])
                            )
                        else:
                            per_file[key][id_val] = el.sourceline

            except (lxml.etree.XMLSyntaxError, Exception) as exc:
                issues.append("  %s: Error: %s" % (xf.relative_to(self.unpacked_dir), exc))

        if issues:
            print("FAILED - Found %d ID uniqueness violations:" % len(issues))
            for ln in issues:
                print(ln)
            return False
        if self.verbose:
            print("PASSED - All required IDs are unique")
        return True

    # ------------------------------------------------------------------
    # Check: file references (.rels → actual files)
    # ------------------------------------------------------------------

    def validate_file_references(self):
        issues = []
        rels_inventory = list(self.unpacked_dir.rglob("*.rels"))

        if not rels_inventory:
            if self.verbose:
                print("PASSED - No .rels files found")
            return True

        all_content = [
            fp.resolve()
            for fp in self.unpacked_dir.rglob("*")
            if fp.is_file()
            and fp.name != "[Content_Types].xml"
            and not fp.name.endswith(".rels")
        ]

        globally_referenced = set()

        if self.verbose:
            print("Found %d .rels files and %d target files" % (len(rels_inventory), len(all_content)))

        for rf in rels_inventory:
            try:
                rroot = lxml.etree.parse(str(rf)).getroot()
                local_refs = set()
                broken = []

                for rel in rroot.findall(".//ns:Relationship", namespaces={"ns": self.NS_PKG_RELS}):
                    tgt = rel.get("Target")
                    if not tgt or tgt.startswith(("http", "mailto:")):
                        continue
                    if tgt.startswith("/"):
                        candidate = self.unpacked_dir / tgt.lstrip("/")
                    elif rf.name == ".rels":
                        candidate = self.unpacked_dir / tgt
                    else:
                        candidate = rf.parent.parent / tgt

                    try:
                        candidate = candidate.resolve()
                        if candidate.exists() and candidate.is_file():
                            local_refs.add(candidate)
                            globally_referenced.add(candidate)
                        else:
                            broken.append((tgt, rel.sourceline))
                    except (OSError, ValueError):
                        broken.append((tgt, rel.sourceline))

                if broken:
                    rp = rf.relative_to(self.unpacked_dir)
                    for ref, ln in broken:
                        issues.append("  %s: Line %s: Broken reference to %s" % (rp, ln, ref))

            except Exception as exc:
                issues.append("  Error parsing %s: %s" % (rf.relative_to(self.unpacked_dir), exc))

        orphans = set(all_content) - globally_referenced
        for orphan in sorted(orphans):
            issues.append("  Unreferenced file: %s" % orphan.relative_to(self.unpacked_dir))

        if issues:
            print("FAILED - Found %d relationship validation errors:" % len(issues))
            for ln in issues:
                print(ln)
            print(
                "CRITICAL: These errors will cause the document to appear corrupt. "
                "Broken references MUST be fixed, "
                "and unreferenced files MUST be referenced or removed."
            )
            return False
        if self.verbose:
            print("PASSED - All references are valid and all files are properly referenced")
        return True

    # ------------------------------------------------------------------
    # Check: relationship ID cross-references
    # ------------------------------------------------------------------

    def validate_all_relationship_ids(self):
        import lxml.etree

        issues = []
        for xf in self.xml_files:
            if xf.suffix == ".rels":
                continue

            companion = xf.parent / "_rels" / ("%s.rels" % xf.name)
            if not companion.exists():
                continue

            try:
                rroot = lxml.etree.parse(str(companion)).getroot()
                rid_type_map = {}

                for rel in rroot.findall("{%s}Relationship" % self.NS_PKG_RELS):
                    rid = rel.get("Id")
                    rtype = rel.get("Type", "")
                    if rid:
                        if rid in rid_type_map:
                            issues.append(
                                "  %s: Line %d: Duplicate relationship ID '%s' (IDs must be unique)"
                                % (companion.relative_to(self.unpacked_dir), rel.sourceline, rid)
                            )
                        type_label = rtype.rsplit("/", 1)[-1] if "/" in rtype else rtype
                        rid_type_map[rid] = type_label

                xroot = lxml.etree.parse(str(xf)).getroot()
                r_ns = self.NS_OFFICE_RELS
                check_attrs = ("id", "embed", "link")

                for el in xroot.iter():
                    for aname in check_attrs:
                        ref = el.get("{%s}%s" % (r_ns, aname))
                        if not ref:
                            continue
                        xrel = xf.relative_to(self.unpacked_dir)
                        ename = el.tag.split("}")[-1] if "}" in el.tag else el.tag

                        if ref not in rid_type_map:
                            sample = ", ".join(sorted(rid_type_map.keys())[:5])
                            if len(rid_type_map) > 5:
                                sample += "..."
                            issues.append(
                                "  %s: Line %d: <%s> r:%s references non-existent relationship '%s' (valid IDs: %s)"
                                % (xrel, el.sourceline, ename, aname, ref, sample)
                            )
                        elif aname == "id" and self.ELEMENT_RELATIONSHIP_TYPES:
                            expected = self._infer_expected_rel_type(ename)
                            if expected and expected not in rid_type_map[ref].lower():
                                issues.append(
                                    "  %s: Line %d: <%s> references '%s' which points to '%s' but should point to a '%s' relationship"
                                    % (xrel, el.sourceline, ename, ref, rid_type_map[ref], expected)
                                )

            except Exception as exc:
                issues.append("  Error processing %s: %s" % (xf.relative_to(self.unpacked_dir), exc))

        if issues:
            print("FAILED - Found %d relationship ID reference errors:" % len(issues))
            for ln in issues:
                print(ln)
            print("\nThese ID mismatches will cause the document to appear corrupt!")
            return False
        if self.verbose:
            print("PASSED - All relationship ID references are valid")
        return True

    def _infer_expected_rel_type(self, elem_name):
        lower = elem_name.lower()
        if lower in self.ELEMENT_RELATIONSHIP_TYPES:
            return self.ELEMENT_RELATIONSHIP_TYPES[lower]

        if lower.endswith("id") and len(lower) > 2:
            prefix = lower[:-2]
            if prefix.endswith("master") or prefix.endswith("layout"):
                return prefix
            return "slide" if prefix == "sld" else prefix

        if lower.endswith("reference") and len(lower) > 9:
            return lower[:-9]

        return None

    # ------------------------------------------------------------------
    # Check: [Content_Types].xml completeness
    # ------------------------------------------------------------------

    def validate_content_types(self):
        issues = []
        ct_fp = self.unpacked_dir / "[Content_Types].xml"

        if not ct_fp.exists():
            print("FAILED - [Content_Types].xml file not found")
            return False

        try:
            root = lxml.etree.parse(str(ct_fp)).getroot()
            declared_parts = set()
            declared_exts = set()

            for ovr in root.findall("{%s}Override" % self.NS_CONTENT_TYPES):
                pn = ovr.get("PartName")
                if pn:
                    declared_parts.add(pn.lstrip("/"))

            for dflt in root.findall("{%s}Default" % self.NS_CONTENT_TYPES):
                ext = dflt.get("Extension")
                if ext:
                    declared_exts.add(ext.lower())

            important_roots = {
                "sld", "sldLayout", "sldMaster", "presentation",
                "document", "workbook", "worksheet", "theme",
            }

            known_media = {
                "png": "image/png", "jpg": "image/jpeg", "jpeg": "image/jpeg",
                "gif": "image/gif", "bmp": "image/bmp", "tiff": "image/tiff",
                "wmf": "image/x-wmf", "emf": "image/x-emf",
            }

            every_file = [fp for fp in self.unpacked_dir.rglob("*") if fp.is_file()]

            for xf in self.xml_files:
                rel = str(xf.relative_to(self.unpacked_dir)).replace("\\", "/")
                if any(s in rel for s in (".rels", "[Content_Types]", "docProps/", "_rels/")):
                    continue
                try:
                    tag = lxml.etree.parse(str(xf)).getroot().tag
                    root_name = tag.split("}")[-1] if "}" in tag else tag
                    if root_name in important_roots and rel not in declared_parts:
                        issues.append("  %s: File with <%s> root not declared in [Content_Types].xml" % (rel, root_name))
                except Exception:
                    continue

            for fp in every_file:
                if fp.suffix.lower() in (".xml", ".rels"):
                    continue
                if fp.name == "[Content_Types].xml":
                    continue
                if "_rels" in fp.parts or "docProps" in fp.parts:
                    continue
                ext = fp.suffix.lstrip(".").lower()
                if ext and ext not in declared_exts and ext in known_media:
                    rp = fp.relative_to(self.unpacked_dir)
                    issues.append(
                        '  %s: File with extension \'%s\' not declared in [Content_Types].xml - should add: <Default Extension="%s" ContentType="%s"/>'
                        % (rp, ext, ext, known_media[ext])
                    )

        except Exception as exc:
            issues.append("  Error parsing [Content_Types].xml: %s" % exc)

        if issues:
            print("FAILED - Found %d content type declaration errors:" % len(issues))
            for ln in issues:
                print(ln)
            return False
        if self.verbose:
            print("PASSED - All content files are properly declared in [Content_Types].xml")
        return True

    # ------------------------------------------------------------------
    # XSD validation helpers
    # ------------------------------------------------------------------

    def validate_file_against_xsd(self, xml_file, verbose=False):
        xf = Path(xml_file).resolve()
        base = self.unpacked_dir.resolve()

        ok, cur_errs = self._check_single_xsd(xf, base)

        if ok is None:
            return None, set()
        if ok:
            return True, set()

        orig_errs = self._original_file_errors(xf)
        assert cur_errs is not None
        new = cur_errs - orig_errs
        new = {e for e in new if not any(p in e for p in self.SKIPPABLE_ERROR_PATTERNS)}

        if new:
            if verbose:
                rp = xf.relative_to(base)
                print("FAILED - %s: %d new error(s)" % (rp, len(new)))
                for e in list(new)[:3]:
                    print("  - %s" % (e[:250] + "..." if len(e) > 250 else e))
            return False, new
        if verbose:
            print("PASSED - No new errors (original had %d errors)" % len(cur_errs))
        return True, set()

    def validate_against_xsd(self):
        fresh_errors = []
        orig_err_ct = 0
        ok_ct = 0
        skip_ct = 0

        for xf in self.xml_files:
            rp = str(xf.relative_to(self.unpacked_dir))
            ok, new_errs = self.validate_file_against_xsd(xf, verbose=False)

            if ok is None:
                skip_ct += 1
            elif ok and not new_errs:
                ok_ct += 1
            elif ok:
                orig_err_ct += 1
                ok_ct += 1
            else:
                fresh_errors.append("  %s: %d new error(s)" % (rp, len(new_errs)))
                for e in list(new_errs)[:3]:
                    fresh_errors.append("    - %s" % (e[:250] + "..." if len(e) > 250 else e))

        if self.verbose:
            print("Validated %d files:" % len(self.xml_files))
            print("  - Valid: %d" % ok_ct)
            print("  - Skipped (no schema): %d" % skip_ct)
            if orig_err_ct:
                print("  - With original errors (ignored): %d" % orig_err_ct)
            count_with_new = len([e for e in fresh_errors if not e.startswith("    ")])
            print("  - With NEW errors: %d" % (count_with_new if fresh_errors else 0))

        if fresh_errors:
            print("\nFAILED - Found NEW validation errors:")
            for ln in fresh_errors:
                print(ln)
            return False
        if self.verbose:
            print("\nPASSED - No new XSD validation errors introduced")
        return True

    # ------------------------------------------------------------------
    # Internal XSD helpers
    # ------------------------------------------------------------------

    def _resolve_schema(self, xf):
        if xf.name in self.XSD_LOOKUP:
            return self.schemas_dir / self.XSD_LOOKUP[xf.name]
        if xf.suffix == ".rels":
            return self.schemas_dir / self.XSD_LOOKUP[".rels"]
        if "charts/" in str(xf) and xf.name.startswith("chart"):
            return self.schemas_dir / self.XSD_LOOKUP["chart"]
        if "theme/" in str(xf) and xf.name.startswith("theme"):
            return self.schemas_dir / self.XSD_LOOKUP["theme"]
        if xf.parent.name in self.CONTENT_ROOT_DIRS:
            return self.schemas_dir / self.XSD_LOOKUP[xf.parent.name]
        return None

    # Keep old name as alias
    _get_schema_path = _resolve_schema

    def _purge_non_ooxml_attrs(self, doc):
        raw = lxml.etree.tostring(doc, encoding="unicode")
        copy = lxml.etree.fromstring(raw)

        for el in copy.iter():
            to_drop = [
                a for a in el.attrib
                if "{" in a and a.split("}")[0][1:] not in self.STANDARD_OOXML_NS
            ]
            for a in to_drop:
                del el.attrib[a]

        self._purge_extension_elements(copy)
        return lxml.etree.ElementTree(copy)

    # Keep old name as alias
    _clean_ignorable_namespaces = _purge_non_ooxml_attrs

    def _purge_extension_elements(self, root):
        removable = []
        for child in list(root):
            if not hasattr(child, "tag") or callable(child.tag):
                continue
            ts = str(child.tag)
            if ts.startswith("{"):
                ns = ts.split("}")[0][1:]
                if ns not in self.STANDARD_OOXML_NS:
                    removable.append(child)
                    continue
            self._purge_extension_elements(child)
        for child in removable:
            root.remove(child)

    # Keep old name as alias
    _remove_ignorable_elements = _purge_extension_elements

    def _strip_mc_ignorable(self, doc):
        root = doc.getroot()
        mc_key = "{%s}Ignorable" % self.NS_MC
        if mc_key in root.attrib:
            del root.attrib[mc_key]
        return doc

    # Keep old name as alias
    _preprocess_for_mc_ignorable = _strip_mc_ignorable

    def _check_single_xsd(self, xf, base):
        schema_fp = self._resolve_schema(xf)
        if schema_fp is None:
            return None, None

        try:
            with open(schema_fp, "rb") as fh:
                xsd_tree = lxml.etree.parse(fh, parser=lxml.etree.XMLParser(), base_url=str(schema_fp))
                schema = lxml.etree.XMLSchema(xsd_tree)

            with open(xf, "r") as fh:
                xml_tree = lxml.etree.parse(fh)

            xml_tree, _ = self._scrub_template_markers(xml_tree)
            xml_tree = self._strip_mc_ignorable(xml_tree)

            rp = xf.relative_to(base)
            if rp.parts and rp.parts[0] in self.CONTENT_ROOT_DIRS:
                xml_tree = self._purge_non_ooxml_attrs(xml_tree)

            if schema.validate(xml_tree):
                return True, set()
            return False, {err.message for err in schema.error_log}

        except Exception as exc:
            return False, {str(exc)}

    # Keep old name as alias
    _validate_single_file_xsd = _check_single_xsd

    def _original_file_errors(self, xf):
        if self.original_file is None:
            return set()

        import tempfile
        import zipfile

        xf = Path(xf).resolve()
        base = self.unpacked_dir.resolve()
        rp = xf.relative_to(base)

        with tempfile.TemporaryDirectory() as td:
            tp = Path(td)
            with zipfile.ZipFile(self.original_file, "r") as zf:
                zf.extractall(tp)
            orig_xf = tp / rp
            if not orig_xf.exists():
                return set()
            _, errs = self._check_single_xsd(orig_xf, tp)
            return errs if errs else set()

    # Keep old name as alias
    _get_original_file_errors = _original_file_errors

    def _scrub_template_markers(self, doc):
        warnings = []
        pattern = re.compile(r"\{\{[^}]*\}\}")

        raw = lxml.etree.tostring(doc, encoding="unicode")
        copy = lxml.etree.fromstring(raw)

        def _clean(txt, label):
            if not txt:
                return txt
            hits = list(pattern.finditer(txt))
            if hits:
                for m in hits:
                    warnings.append("Found template tag in %s: %s" % (label, m.group()))
                return pattern.sub("", txt)
            return txt

        for el in copy.iter():
            if not hasattr(el, "tag") or callable(el.tag):
                continue
            ts = str(el.tag)
            if ts.endswith("}t") or ts == "t":
                continue
            el.text = _clean(el.text, "text content")
            el.tail = _clean(el.tail, "tail content")

        return lxml.etree.ElementTree(copy), warnings

    # Keep old name as alias
    _remove_template_tags_from_text_nodes = _scrub_template_markers


if __name__ == "__main__":
    raise RuntimeError("This module should not be run directly.")
