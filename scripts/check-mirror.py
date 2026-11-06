#!/usr/bin/env python3
"""Check that a public source mirror is incorporated in the private source."""
import argparse
import json
import subprocess
from pathlib import Path


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def inspect(source, public):
    source = Path(source).resolve()
    public = Path(public).resolve()
    if source == public:
        raise ValueError("Source and public checkouts must be different.")
    for root in (source, public):
        if git(root, "status", "--porcelain"):
            raise ValueError("Commit or set aside local changes before checking the mirror.")
        if git(root, "symbolic-ref", "--short", "HEAD") != "main":
            raise ValueError("Both checkouts must be on main.")
        remote_head = git(root, "ls-remote", "--exit-code", "origin", "refs/heads/main").split()[0]
        if git(root, "rev-parse", "HEAD") != remote_head:
            raise ValueError("Both local main branches must match their origin main. Push private changes first; incorporate remote changes before publishing.")
    public_tree = git(public, "rev-parse", "HEAD^{tree}")
    source_tree = git(source, "rev-parse", "HEAD^{tree}")
    incorporated = next((line.split()[0] for line in git(source, "log", "--format=%H %T").splitlines()
                         if line.split()[1] == public_tree), None)
    if not incorporated:
        raise ValueError("Public HEAD is not incorporated in private history. Import and review public changes in private, commit and push them, then retry.")
    return {"ready": True, "identical": source_tree == public_tree,
            "source_head": git(source, "rev-parse", "HEAD"),
            "public_head": git(public, "rev-parse", "HEAD"),
            "incorporated_at": incorporated,
            "source_tree": source_tree, "public_tree": public_tree}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--public", required=True, type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(inspect(args.source, args.public), indent=2))
    except (ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Mirror check failed: {error}\n")


if __name__ == "__main__":
    main()
