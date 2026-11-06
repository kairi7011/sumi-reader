import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("mirror", Path(__file__).resolve().parents[1] / "scripts/check-mirror.py")
mirror = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mirror)


class MirrorTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.create_repo("private")
        self.public = self.create_repo("public")

    def git(self, root, *args):
        return subprocess.check_output(["git", "-C", str(root), *args], stderr=subprocess.DEVNULL, text=True).strip()

    def create_repo(self, name):
        remote = self.root / f"{name}.git"
        remote.mkdir()
        self.git(remote, "init", "--bare", "--initial-branch=main")
        repo = self.root / name
        repo.mkdir()
        self.git(repo, "init", "--initial-branch=main")
        self.git(repo, "config", "user.name", "Fixture")
        self.git(repo, "config", "user.email", "fixture@users.noreply.github.com")
        self.git(repo, "remote", "add", "origin", str(remote))
        self.commit(repo, "initial", name)
        return repo

    def commit(self, repo, contents, message="Change", push=True):
        (repo / "source.txt").write_text(contents)
        self.git(repo, "add", "source.txt")
        self.git(repo, "commit", "-m", message)
        if push:
            self.git(repo, "push", "origin", "main")

    def test_separate_histories_with_same_tree_are_synced(self):
        self.assertNotEqual(self.git(self.source, "rev-parse", "HEAD"), self.git(self.public, "rev-parse", "HEAD"))
        self.assertTrue(mirror.inspect(self.source, self.public)["identical"])

    def test_pushed_private_work_is_ready_for_export(self):
        self.commit(self.source, "private first")
        result = mirror.inspect(self.source, self.public)
        self.assertTrue(result["ready"])
        self.assertFalse(result["identical"])

    def test_public_ahead_requires_private_catchup(self):
        self.commit(self.public, "public contribution")
        with self.assertRaisesRegex(ValueError, "not incorporated"):
            mirror.inspect(self.source, self.public)
        self.commit(self.source, "public contribution", "Incorporate public change")
        self.assertTrue(mirror.inspect(self.source, self.public)["identical"])

    def test_unpushed_private_work_cannot_be_published(self):
        self.commit(self.source, "local only", push=False)
        with self.assertRaisesRegex(ValueError, "Push private changes first"):
            mirror.inspect(self.source, self.public)

    def test_dirty_checkouts_are_rejected(self):
        (self.public / "source.txt").write_text("uncommitted")
        with self.assertRaisesRegex(ValueError, "local changes"):
            mirror.inspect(self.source, self.public)


if __name__ == "__main__":
    unittest.main()
