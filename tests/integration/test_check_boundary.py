import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT_PATH = Path(__file__).resolve().parents[2] / "scripts" / "check_boundary.py"
SPEC = importlib.util.spec_from_file_location("check_boundary", SCRIPT_PATH)
check_boundary = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
# Registered before it is executed: the dataclasses in it look their own
# module up by name while the decorator runs.
sys.modules["check_boundary"] = check_boundary
SPEC.loader.exec_module(check_boundary)

Debt = check_boundary.Debt
Product = check_boundary.Product


def read(source: str, name: str = "snippet.rs"):
    """Run the Rust reader over a snippet, as the check does over a file.

    `name` matters: the reader decides from it whether the whole file is one
    module's test body moved out of line.
    """
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / name
        path.write_text(source, encoding="utf-8")
        return check_boundary.read_rust(str(path))


def literals(files, products, ledger=()):
    """The literal check with an explicit product list and ledger."""
    original_products = check_boundary.PRODUCTS
    original_ledger = check_boundary.LEDGER
    check_boundary.PRODUCTS = tuple(products)
    check_boundary.LEDGER = tuple(ledger)
    try:
        return check_boundary.check_literals(files)
    finally:
        check_boundary.PRODUCTS = original_products
        check_boundary.LEDGER = original_ledger


class ReadRustTests(unittest.TestCase):
    """The reader decides what the law even looks at, so it is what to trust."""

    def test_comments_are_documentation_and_do_not_count(self):
        lines = read('/// gh:acme/widget#42\n// gh again\nlet a = 1;\n')
        self.assertNotIn("gh", lines[0].code)
        self.assertNotIn("gh", lines[1].code)
        self.assertIn("let a = 1;", lines[2].code)

    def test_a_url_in_a_string_is_not_mistaken_for_a_comment(self):
        lines = read('let url = "https://forge.example/a/b";\n')
        self.assertIn("//forge.example/a/b", lines[0].code)

    def test_a_brace_inside_a_string_does_not_move_the_scope(self):
        source = (
            '#[cfg(test)]\n'
            'mod tests {\n'
            '    let brace = "{";\n'
            '    let inside = 1;\n'
            '}\n'
            'let outside = 2;\n'
        )
        lines = read(source)
        by_number = {line.number: line for line in lines}
        self.assertTrue(by_number[4].in_test, "the test body is a test body")
        self.assertFalse(by_number[6].in_test, "and it ends where it ends")

    def test_a_sibling_tests_file_is_a_test_body_all_the_way_down(self):
        # Its `#[cfg(test)]` sits on the attachment in the parent, so the file
        # itself carries no marker: the name is what says it.
        lines = read('use super::*;\nlet name = "widget";\n', "mod_tests.rs")
        self.assertTrue(all(line.in_test for line in lines))

    def test_a_file_that_is_not_one_still_has_its_bodies_tracked(self):
        source = '#[cfg(test)]\nmod tests {\n    let a = 1;\n}\nlet b = 2;\n'
        by_number = {line.number: line for line in read(source, "engine.rs")}
        self.assertTrue(by_number[3].in_test)
        self.assertFalse(by_number[5].in_test)

    def test_a_raw_string_keeps_its_hashes_and_its_newlines(self):
        lines = read('const S: &str = r##"a "quoted" {\nb"##;\nlet after = 1;\n')
        self.assertIn('a "quoted" {', lines[0].code)
        self.assertIn("let after = 1;", lines[2].code)

    def test_a_block_comment_spanning_lines_is_dropped(self):
        lines = read("/* gh\n   gh */ let a = 1;\n")
        self.assertNotIn("gh", lines[0].code)
        self.assertIn("let a = 1;", lines[1].code)

    def test_an_escaped_quote_ends_its_own_char_literal(self):
        # `'\\''` closes on its fourth character; reading past it swallowed
        # the rest of the file, comments included.
        lines = read("let q = value.replace('\\\\'', \"x\");\n/// gh\nlet a = 1;\n")
        self.assertNotIn("gh", lines[1].code)
        self.assertIn("let a = 1;", lines[2].code)

    def test_a_lifetime_is_not_an_unterminated_char_literal(self):
        lines = read("fn f<'a>(x: &'a str) -> &'a str { x }\nlet after = 1;\n")
        self.assertIn("&'a str", lines[0].code)
        self.assertIn("let after = 1;", lines[1].code)


class LiteralConfinementTests(unittest.TestCase):
    WIDGET = Product("widget", r"widget", homes=("src/adapters/widget.rs",))

    def test_a_name_outside_its_adapter_is_a_finding(self):
        files = {"src/engine.rs": read('let name = "widget";\n')}
        findings, stale = literals(files, [self.WIDGET])
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].path, "src/engine.rs")
        self.assertEqual(stale, [])

    def test_the_same_name_inside_its_adapter_is_not(self):
        files = {"src/adapters/widget.rs": read('let name = "widget";\n')}
        findings, _ = literals(files, [self.WIDGET])
        self.assertEqual(findings, [])

    def test_a_fixture_is_an_example_and_is_allowed(self):
        source = '#[cfg(test)]\nmod tests {\n    let name = "widget";\n}\n'
        files = {"src/engine.rs": read(source)}
        findings, _ = literals(files, [self.WIDGET])
        self.assertEqual(findings, [])

    def test_a_fixture_the_module_moved_to_a_sibling_is_still_an_example(self):
        source = 'use super::*;\nlet name = "widget";\n'
        files = {"src/engine_tests.rs": read(source, "engine_tests.rs")}
        findings, _ = literals(files, [self.WIDGET])
        self.assertEqual(findings, [])

    def test_the_ledger_excuses_one_spelling_and_not_the_file(self):
        source = 'let user = widget_user;\nlet name = "widget";\n'
        debt = Debt("src/engine.rs", "widget", r"widget_user", "a schema change")
        files = {"src/engine.rs": read(source)}
        findings, stale = literals(files, [self.WIDGET], [debt])
        self.assertEqual([finding.number for finding in findings], [2])
        self.assertEqual(stale, [])

    def test_a_ledger_entry_that_matches_nothing_is_itself_an_error(self):
        debt = Debt("src/engine.rs", "widget", r"widget_user", "a schema change")
        files = {"src/engine.rs": read("let a = 1;\n")}
        findings, stale = literals(files, [self.WIDGET], [debt])
        self.assertEqual(findings, [])
        self.assertEqual(len(stale), 1)


class CoreIsIoFreeTests(unittest.TestCase):
    def setUp(self):
        self.original = check_boundary.CORE
        check_boundary.CORE = ("pure", "also_pure")

    def tearDown(self):
        check_boundary.CORE = self.original

    def core(self, files):
        return check_boundary.check_core(files)

    def test_a_pure_module_passes(self):
        files = {
            "src/pure.rs": read("use crate::also_pure::Thing;\nlet a = 1;\n"),
            "src/also_pure.rs": read("pub struct Thing;\n"),
        }
        self.assertEqual(self.core(files), [])

    def test_reaching_the_filesystem_is_a_finding(self):
        files = {
            "src/pure.rs": read("let text = std::fs::read_to_string(path);\n"),
            "src/also_pure.rs": read("pub struct Thing;\n"),
        }
        findings = self.core(files)
        self.assertEqual(len(findings), 1)
        self.assertIn("std::fs", findings[0].what)

    def test_reaching_a_module_above_core_is_a_finding(self):
        files = {
            "src/pure.rs": read("crate::registry::read();\n"),
            "src/also_pure.rs": read("pub struct Thing;\n"),
        }
        findings = self.core(files)
        self.assertEqual(len(findings), 1)
        self.assertIn("crate::registry", findings[0].what)

    def test_a_test_body_may_touch_the_disk(self):
        source = (
            "pub struct Thing;\n"
            "#[cfg(test)]\n"
            "mod tests {\n"
            "    std::fs::create_dir_all(path).unwrap();\n"
            "}\n"
        )
        files = {
            "src/pure.rs": read(source),
            "src/also_pure.rs": read("pub struct Thing;\n"),
        }
        self.assertEqual(self.core(files), [])

    def test_a_core_module_that_is_not_there_is_a_finding(self):
        files = {"src/pure.rs": read("let a = 1;\n")}
        findings = self.core(files)
        self.assertEqual(len(findings), 1)
        self.assertIn("also_pure", findings[0].path)


if __name__ == "__main__":
    unittest.main()
