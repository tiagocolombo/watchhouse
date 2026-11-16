#!/usr/bin/env python3
"""Package/check trusted CI-native test builds. No publishing, installer or product authority."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tempfile
import zipfile

DEMO_FILES = (
    "gallery.db", "incident-example.md", "investigation-example.md",
    "qualification-example.md", "recovery-example.md", "postmortem-example.md",
    "structured-record-example.json",
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def create(args):
    require(re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", args.commit), "full source commit required")
    require(not args.output.exists(), "output already exists; use a fresh path")
    require(args.binary.is_file() and args.binary.stat().st_size <= 512 * 1024 * 1024, "native binary missing/too large")
    require(set(p.name for p in args.demo.iterdir()) == set(DEMO_FILES), "demo must contain exactly the generator's files")
    executable = "watchhouse.exe" if platform.system() == "Windows" else "watchhouse"
    with tempfile.TemporaryDirectory(prefix="watchhouse-package-") as temporary:
        stage = Path(temporary)
        shutil.copy2(args.binary, stage / executable)
        (stage / "demo").mkdir()
        for name in DEMO_FILES:
            source = args.demo / name
            require(source.is_file() and not source.is_symlink() and source.stat().st_size <= 32 * 1024 * 1024, "invalid demo file")
            shutil.copy2(source, stage / "demo" / name)
        shutil.copy2(Path(__file__).resolve().parents[1] / "TESTING.md", stage / "TESTING.md")
        files = sorted([executable, "TESTING.md", *(f"demo/{name}" for name in DEMO_FILES)])
        manifest = {
            "schema_version": 1, "scope": "native-coordinator-test-build-v1",
            "source_commit": args.commit, "system": platform.system(),
            "machine": platform.machine(), "build_platform": platform.platform(),
            "binary": executable, "profile": "debug", "real_execution_admitted": False,
            "demo": "synthetic-only", "files": {name: sha((stage / name).read_bytes()) for name in files},
        }
        (stage / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        files.append("manifest.json")
        checksums = "".join(f"{sha((stage / name).read_bytes())}  {name}\n" for name in sorted(files))
        (stage / "SHA256SUMS").write_text(checksums, encoding="utf-8", newline="\n")
        files.append("SHA256SUMS")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        # Exclusive creation prevents accidental replacement even if another process wins the race.
        with zipfile.ZipFile(args.output, "x", zipfile.ZIP_DEFLATED) as archive:
            for name in sorted(files):
                archive.write(stage / name, name)
    print(args.output)


def check(args):
    # Run only archives produced from trusted source: hashes are integrity checks, not signatures.
    with zipfile.ZipFile(args.bundle) as archive, tempfile.TemporaryDirectory(prefix="watchhouse-bundle-check-") as temporary:
        require(len(archive.infolist()) <= 20, "unexpected archive size")
        require(sum(i.file_size for i in archive.infolist()) <= 1024 ** 3, "archive exceeds bound")
        require(len(archive.namelist()) == len(set(archive.namelist())), "duplicate archive entry")
        manifest = json.loads(archive.read("manifest.json"))
        require(manifest["schema_version"] == 1 and manifest["scope"] == "native-coordinator-test-build-v1", "unsupported bundle")
        require(manifest["system"] == platform.system() and manifest["machine"] == platform.machine(), "bundle does not match this native platform")
        executable = "watchhouse.exe" if platform.system() == "Windows" else "watchhouse"
        require(manifest["binary"] == executable and manifest["real_execution_admitted"] is False and manifest["demo"] == "synthetic-only", "invalid bundle scope")
        files = {executable, "TESTING.md", *(f"demo/{name}" for name in DEMO_FILES)}
        require(set(manifest["files"]) == files, "unexpected manifest paths")
        require(set(archive.namelist()) == files | {"manifest.json", "SHA256SUMS"}, "unexpected archive paths")
        for name in files:
            require(sha(archive.read(name)) == manifest["files"][name], f"digest mismatch: {name}")
        expected = "".join(f"{sha(archive.read(name))}  {name}\n" for name in sorted(files | {"manifest.json"}))
        require(archive.read("SHA256SUMS").decode("utf-8") == expected, "checksum manifest changed")
        root = Path(temporary)
        # Write only validated fixed names; never extract arbitrary archive paths/symlinks.
        for name in files | {"manifest.json", "SHA256SUMS"}:
            target = root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(name))
        binary = root / executable
        if os.name != "nt":
            binary.chmod(0o755)
        version = subprocess.run([str(binary), "--version"], cwd=root, check=True, capture_output=True, text=True, timeout=30).stdout
        require(version.startswith("watchhouse "), "packaged version failed")
        base = [str(binary), "--db", str(root / "demo" / "gallery.db")]
        cases = json.loads(subprocess.run([*base, "case", "list"], cwd=root, check=True, capture_output=True, text=True, timeout=30).stdout)
        require(len(cases) == 1, "demo must have one inspectable synthetic incident")
        snapshot = json.loads(subprocess.run([*base, "case", "show", cases[0]["id"]], cwd=root, check=True, capture_output=True, text=True, timeout=30).stdout)
        require(snapshot["recovery"]["recovery"]["response_resolved"], "recovery scenario missing")
        require(any(a["state"] == "open" for a in snapshot["recovery"]["actions"]), "continuing corrective action missing")
        require(len([e for e in snapshot["learning"]["entries"] if e["accepted_current"]]) >= 2, "accepted postmortem/knowledge scenario missing or stale")
        require(all(result["actual_model"] is None for result in snapshot["results"]), "unexpected live model result")
        print(f"Verified native test bundle: {manifest['system']} {manifest['machine']} @ {manifest['source_commit']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    build = sub.add_parser("create")
    build.add_argument("--binary", type=Path, required=True)
    build.add_argument("--demo", type=Path, required=True)
    build.add_argument("--commit", required=True)
    build.add_argument("--output", type=Path, required=True)
    verify = sub.add_parser("check")
    verify.add_argument("--bundle", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "create":
        create(args)
    else:
        check(args)


if __name__ == "__main__":
    main()
