#!/usr/bin/env python3
"""Check tracked source, reachable Git history, and optional release contents."""
import argparse
import json
import re
import subprocess
from pathlib import Path

RULES = {
    "private-home-path": re.compile(rb"/(?:Us" + rb"ers|ho" + rb"me)/[A-Za-z0-9_.-]+/"),
    "private-temporary-path": re.compile(rb"/var/fol" + rb"ders/[A-Za-z0-9_-]+/"),
    "private-key": re.compile(rb"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    "github-token": re.compile(rb"(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{60,})"),
    "service-token": re.compile(rb"(?:xox[baprs]-[0-9A-Za-z-]{30,}|sk-(?:proj-)?[A-Za-z0-9_-]{40,}|AKIA[A-Z0-9]{16})"),
}
PRIVATE_NAMES = {".env", ".native-toolchain.json", "library.json"}
PRIVATE_SUFFIXES = {".pdf", ".epub", ".sqlite", ".sqlite3", ".db", ".pem", ".p12", ".pfx", ".key"}


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args])


def inspect(root, history=False, bundle=None):
    findings = []
    counts = {"tracked_files": 0, "history_blobs": 0, "commits": 0, "bundle_files": 0}

    def scan(data, location):
        for rule, pattern in RULES.items():
            if pattern.search(data):
                findings.append({"rule": rule, "location": location})

    for name in git(root, "ls-files", "-z").decode().split("\0"):
        if not name:
            continue
        p = root / name
        counts["tracked_files"] += 1
        if p.name in PRIVATE_NAMES or p.suffix.lower() in PRIVATE_SUFFIXES:
            findings.append({"rule": "private-data-file", "location": name})
        if p.is_symlink():
            findings.append({"rule": "source-symlink", "location": name})
        elif p.exists():
            scan(p.read_bytes(), name)

    if history:
        objects = git(root, "rev-list", "--objects", "--all", "--no-object-names").splitlines()
        process = subprocess.Popen(["git", "-C", str(root), "cat-file", "--batch"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        try:
            for oid in objects:
                process.stdin.write(oid + b"\n")
                process.stdin.flush()
                header = process.stdout.readline().split()
                if len(header) != 3:
                    raise RuntimeError("Could not inspect a reachable Git object")
                data = process.stdout.read(int(header[2]))
                process.stdout.read(1)
                location = "git:" + oid.decode()[:12]
                if header[1] == b"blob":
                    counts["history_blobs"] += 1
                    scan(data, location)
                elif header[1] == b"commit":
                    counts["commits"] += 1
                    scan(data, location)
                    for line in data.split(b"\n\n", 1)[0].splitlines():
                        if line.startswith((b"author ", b"committer ")):
                            email = re.search(rb"<([^>]+)>", line)
                            if not email or not email[1].endswith(b"@users.noreply.github.com"):
                                findings.append({"rule": "public-author-email-required", "location": location})
        finally:
            process.stdin.close()
            process.stdout.close()
            if process.wait() != 0:
                raise RuntimeError("Git history inspection failed")

    if bundle:
        for p in sorted(bundle.rglob("*")):
            if p.is_symlink():
                findings.append({"rule": "bundle-symlink", "location": str(p.relative_to(bundle))})
            elif p.is_file():
                counts["bundle_files"] += 1
                scan(p.read_bytes(), "bundle:" + str(p.relative_to(bundle)))
                if p.name in PRIVATE_NAMES or p.suffix.lower() in PRIVATE_SUFFIXES:
                    findings.append({"rule": "private-data-file", "location": "bundle:" + str(p.relative_to(bundle))})
        for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
            if not (bundle / "Contents/Resources" / name).is_file():
                findings.append({"rule": "missing-license", "location": name})
    return {"counts": counts, "passed": not findings, "findings": findings,
            "limits": "Pattern checks are not a complete secret, copyright, vulnerability, or security audit. Inspect new images and fixtures separately."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--history", action="store_true")
    parser.add_argument("--bundle", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    report = inspect(args.repo.resolve(), args.history, args.bundle)
    encoded = json.dumps(report, indent=2) + "\n"
    if args.report:
        args.report.write_text(encoded)
    print(encoded, end="")
    raise SystemExit(0 if report["passed"] else 1)


if __name__ == "__main__":
    main()
