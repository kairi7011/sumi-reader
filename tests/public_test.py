import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("public_check", Path(__file__).resolve().parents[1] / "scripts/check-public.py")
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class PublicDistribution(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.name", "Example")
        self.git("config", "user.email", "example@users.noreply.github.com")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args], stderr=subprocess.STDOUT)

    def commit(self, text):
        (self.root / "note.txt").write_text(text)
        self.git("add", "note.txt")
        self.git("commit", "-qm", "Record fixture")

    def test_deleted_secret_is_still_rejected_in_history(self):
        self.commit("ghp_" + "A" * 36)
        self.commit("No private data in the latest version")
        self.assertTrue(check.inspect(self.root)["passed"])
        result = check.inspect(self.root, history=True)
        self.assertFalse(result["passed"])
        self.assertIn("github-token", {row["rule"] for row in result["findings"]})

    def test_author_email_must_be_the_public_identity(self):
        self.git("config", "user.email", "person@example.org")
        self.commit("A synthetic note")
        result = check.inspect(self.root, history=True)
        self.assertIn("public-author-email-required", {row["rule"] for row in result["findings"]})

    def test_deleted_private_document_is_rejected_without_a_secret_pattern(self):
        document = self.root / "private.pdf"
        document.write_bytes(b"%PDF-1.7\nSynthetic document without credential or path patterns")
        self.git("add", "private.pdf")
        self.git("commit", "-qm", "Add synthetic document")
        document.unlink()
        self.git("add", "-u")
        self.git("commit", "-qm", "Remove synthetic document")
        self.assertTrue(check.inspect(self.root)["passed"])
        result = check.inspect(self.root, history=True)
        findings = [row for row in result["findings"] if row["rule"] == "private-data-file"]
        self.assertTrue(findings)
        self.assertTrue(all(row["location"].startswith("git:") and "private.pdf" not in row["location"] for row in findings))

    def test_deleted_symlink_is_rejected_without_following_its_target(self):
        link = self.root / "shortcut"
        link.symlink_to("missing-synthetic-target")
        self.git("add", "shortcut")
        self.git("commit", "-qm", "Add synthetic symlink")
        link.unlink()
        self.git("add", "-u")
        self.git("commit", "-qm", "Remove synthetic symlink")
        self.assertTrue(check.inspect(self.root)["passed"])
        result = check.inspect(self.root, history=True)
        self.assertIn("source-symlink", {row["rule"] for row in result["findings"]})

    def test_binary_paths_and_missing_bundle_notices_are_rejected(self):
        self.commit("A synthetic note")
        bundle = self.root / "app"
        bundle.mkdir()
        (bundle / "binary").write_bytes(b"\0" + b"/Us" + b"ers/example/project/file.rs\0")
        result = check.inspect(self.root, bundle=bundle)
        rules = {row["rule"] for row in result["findings"]}
        self.assertIn("private-home-path", rules)
        self.assertIn("missing-license", rules)

    def test_clean_history_and_licensed_bundle_pass(self):
        self.commit("Original sample document")
        bundle = self.root / "app"
        resources = bundle / "Contents/Resources"
        resources.mkdir(parents=True)
        for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
            (resources / name).write_text("Synthetic license fixture")
        result = check.inspect(self.root, history=True, bundle=bundle)
        self.assertTrue(result["passed"], result["findings"])
        self.assertEqual(result["counts"]["commits"], 1)


if __name__ == "__main__":
    unittest.main()
