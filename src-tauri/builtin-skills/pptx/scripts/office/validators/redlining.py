"""
Tracked-change consistency validator for Word documents.
"""

import subprocess
import tempfile
import zipfile
from pathlib import Path


class RedliningValidator:

    def __init__(self, unpacked_dir, original_docx, verbose=False, author="Claude"):
        self.pkg_dir = Path(unpacked_dir)
        self.orig_docx = Path(original_docx)
        self.verbose = verbose
        self.author = author
        self._ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}

    def repair(self) -> int:
        return 0

    def validate(self):
        mod_xml = self.pkg_dir / "word" / "document.xml"
        if not mod_xml.exists():
            print("FAILED - Modified document.xml not found at %s" % mod_xml)
            return False

        # Quick check: if no tracked changes by target author, skip heavy comparison
        try:
            import xml.etree.ElementTree as ET
            tree = ET.parse(mod_xml)
            root = tree.getroot()
            w_ns = self._ns["w"]
            author_key = "{%s}author" % w_ns

            dels = [e for e in root.findall(".//{%s}del" % w_ns) if e.get(author_key) == self.author]
            inserts = [e for e in root.findall(".//{%s}ins" % w_ns) if e.get(author_key) == self.author]

            if not dels and not inserts:
                if self.verbose:
                    print("PASSED - No tracked changes by %s found." % self.author)
                return True
        except Exception:
            pass

        with tempfile.TemporaryDirectory() as td:
            tp = Path(td)

            try:
                with zipfile.ZipFile(self.orig_docx, "r") as zf:
                    zf.extractall(tp)
            except Exception as exc:
                print("FAILED - Error unpacking original docx: %s" % exc)
                return False

            orig_xml = tp / "word" / "document.xml"
            if not orig_xml.exists():
                print("FAILED - Original document.xml not found in %s" % self.orig_docx)
                return False

            try:
                import xml.etree.ElementTree as ET
                mod_root = ET.parse(mod_xml).getroot()
                orig_root = ET.parse(orig_xml).getroot()
            except ET.ParseError as exc:
                print("FAILED - Error parsing XML files: %s" % exc)
                return False

            self._strip_author_changes(orig_root)
            self._strip_author_changes(mod_root)

            mod_text = self._flatten_text(mod_root)
            orig_text = self._flatten_text(orig_root)

            if mod_text != orig_text:
                print(self._build_diff_report(orig_text, mod_text))
                return False

            if self.verbose:
                print("PASSED - All changes by %s are properly tracked" % self.author)
            return True

    # ------------------------------------------------------------------
    # Diff report generation
    # ------------------------------------------------------------------

    def _build_diff_report(self, orig_text, mod_text):
        parts = [
            "FAILED - Document text doesn't match after removing %s's tracked changes" % self.author,
            "",
            "Likely causes:",
            "  1. Modified text inside another author's <w:ins> or <w:del> tags",
            "  2. Made edits without proper tracked changes",
            "  3. Didn't nest <w:del> inside <w:ins> when deleting another's insertion",
            "",
            "For pre-redlined documents, use correct patterns:",
            "  - To reject another's INSERTION: Nest <w:del> inside their <w:ins>",
            "  - To restore another's DELETION: Add new <w:ins> AFTER their <w:del>",
            "",
        ]

        diff_output = self._run_git_word_diff(orig_text, mod_text)
        if diff_output:
            parts.extend(["Differences:", "============", diff_output])
        else:
            parts.append("Unable to generate word diff (git not available)")

        return "\n".join(parts)

    def _run_git_word_diff(self, orig_text, mod_text):
        try:
            with tempfile.TemporaryDirectory() as td:
                tp = Path(td)
                (tp / "a.txt").write_text(orig_text, encoding="utf-8")
                (tp / "b.txt").write_text(mod_text, encoding="utf-8")

                for extra_flags in (["--word-diff-regex=."], []):
                    proc = subprocess.run(
                        ["git", "diff", "--word-diff=plain"] + extra_flags +
                        ["-U0", "--no-index", str(tp / "a.txt"), str(tp / "b.txt")],
                        capture_output=True, text=True,
                    )
                    if proc.stdout.strip():
                        lines = proc.stdout.split("\n")
                        body = []
                        after_header = False
                        for ln in lines:
                            if ln.startswith("@@"):
                                after_header = True
                                continue
                            if after_header and ln.strip():
                                body.append(ln)
                        if body:
                            return "\n".join(body)

        except (subprocess.CalledProcessError, FileNotFoundError, Exception):
            pass
        return None

    # ------------------------------------------------------------------
    # Tracked-change stripping
    # ------------------------------------------------------------------

    def _strip_author_changes(self, root):
        w = self._ns["w"]
        ins_tag = "{%s}ins" % w
        del_tag = "{%s}del" % w
        author_attr = "{%s}author" % w

        # Remove author's insertions entirely
        for parent in root.iter():
            removable = [
                ch for ch in parent
                if ch.tag == ins_tag and ch.get(author_attr) == self.author
            ]
            for el in removable:
                parent.remove(el)

        # Promote author's deletions back to normal text
        deltext_tag = "{%s}delText" % w
        t_tag = "{%s}t" % w

        for parent in root.iter():
            targets = [
                (ch, list(parent).index(ch))
                for ch in parent
                if ch.tag == del_tag and ch.get(author_attr) == self.author
            ]
            for del_el, pos in reversed(targets):
                for descendant in del_el.iter():
                    if descendant.tag == deltext_tag:
                        descendant.tag = t_tag
                for child in reversed(list(del_el)):
                    parent.insert(pos, child)
                parent.remove(del_el)

    # ------------------------------------------------------------------
    # Text extraction
    # ------------------------------------------------------------------

    def _flatten_text(self, root) -> str:
        w = self._ns["w"]
        p_tag = "{%s}p" % w
        t_tag = "{%s}t" % w

        paragraphs = []
        for p_el in root.findall(".//%s" % p_tag):
            fragments = [t.text for t in p_el.findall(".//%s" % t_tag) if t.text]
            combined = "".join(fragments)
            if combined:
                paragraphs.append(combined)
        return "\n".join(paragraphs)


if __name__ == "__main__":
    raise RuntimeError("This module should not be run directly.")
