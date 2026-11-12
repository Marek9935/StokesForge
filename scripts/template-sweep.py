#!/usr/bin/env python3
"""Sweep the template target (#523): delete every file the refresh builds did not list.

    template-sweep.py <target-dir> <cargo-json-log>...

Each log holds the stdout of one refresh command run with cargo's JSON messages (lines that are
not JSON are skipped). Kept: the `filenames` and `executable` of every `compiler-artifact`, their
`.d` siblings (same stem), the `.fingerprint/<unit>` directories whose hash belongs to a kept unit,
and the `build/<unit>` directories holding a kept file or a build script `out_dir`. Everything
else in `deps/`, `.fingerprint/` and `build/` of each profile directory is deleted, and
`incremental/` and `target/doc` go entirely: the template never keeps incremental state.

Nothing is deleted when no log lists a single artifact (a failed refresh must not empty the
target): exit 2. Prints one summary line. Stdlib only.
"""
import json
import os
import re
import shutil
import sys

HASH = re.compile(r"-([0-9a-f]{16})$")


def listed(logs):
    files, dirs = set(), set()
    for path in logs:
        with open(path, errors="replace") as f:
            for line in f:
                line = line.strip()
                if not line.startswith("{"):
                    continue
                try:
                    m = json.loads(line)
                except ValueError:
                    continue
                reason = m.get("reason")
                if reason == "compiler-artifact":
                    for p in m.get("filenames") or []:
                        files.add(os.path.normpath(p))
                    if m.get("executable"):
                        files.add(os.path.normpath(m["executable"]))
                elif reason == "build-script-executed" and m.get("out_dir"):
                    dirs.add(os.path.normpath(m["out_dir"]))
    return files, dirs


def hash_of(name):
    """The 16-hex metadata hash at the end of `name` (extension already removed), or None."""
    m = HASH.search(name)
    return m.group(1) if m else None


def size_kb(path):
    total = 0
    if os.path.islink(path) or os.path.isfile(path):
        return os.lstat(path).st_blocks // 2
    for root, _, names in os.walk(path):
        for n in names:
            try:
                total += os.lstat(os.path.join(root, n)).st_blocks // 2
            except OSError:
                pass
    return total


def remove(path):
    kb = size_kb(path)
    if os.path.isdir(path) and not os.path.islink(path):
        shutil.rmtree(path, ignore_errors=True)
    else:
        try:
            os.remove(path)
        except OSError:
            return 0
    return kb


def sweep(target, files, dirs):
    target = os.path.normpath(os.path.abspath(target))
    stems = {os.path.splitext(f)[0] for f in files} | set(files)
    hashes = set()
    for f in files:
        h = hash_of(os.path.splitext(os.path.basename(f))[0])
        if h:
            hashes.add(h)
    keep_build = set()       # build/<unit> directories to keep
    for p in list(files) + list(dirs):
        rel = os.path.relpath(p, target)
        parts = rel.split(os.sep)
        if parts[0] != ".." and len(parts) >= 4 and parts[1] == "build":
            keep_build.add(os.path.join(target, *parts[:3]))
            h = hash_of(parts[2])
            if h:
                hashes.add(h)
    freed, deleted = 0, 0
    for prof in sorted(os.listdir(target)):
        pdir = os.path.join(target, prof)
        if not os.path.isdir(pdir) or os.path.islink(pdir):
            continue
        if prof in ("doc", "nextest", "tmp"):
            if prof == "doc":
                freed += remove(pdir)
                deleted += 1
            continue
        for name in ("incremental", "examples"):
            p = os.path.join(pdir, name)
            if os.path.lexists(p):
                freed += remove(p)
                deleted += 1
        deps = os.path.join(pdir, "deps")
        if os.path.isdir(deps):
            for n in os.listdir(deps):
                p = os.path.join(deps, n)
                if os.path.normpath(p) in files or os.path.splitext(os.path.normpath(p))[0] in stems:
                    continue
                freed += remove(p)
                deleted += 1
        fp = os.path.join(pdir, ".fingerprint")
        if os.path.isdir(fp):
            for n in os.listdir(fp):
                h = hash_of(n)
                if h and h in hashes:
                    continue
                freed += remove(os.path.join(fp, n))
                deleted += 1
        bd = os.path.join(pdir, "build")
        if os.path.isdir(bd):
            for n in os.listdir(bd):
                p = os.path.normpath(os.path.join(bd, n))
                if p in keep_build:
                    continue
                if not os.path.isdir(p):
                    continue       # build/.rustc-info and the like
                freed += remove(p)
                deleted += 1
    return deleted, freed


def main(argv):
    if len(argv) < 3:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        return 2
    target, logs = argv[1], argv[2:]
    files, dirs = listed(logs)
    if not files:
        print("template-sweep: no compiler-artifact in the logs; nothing deleted", file=sys.stderr)
        return 2
    deleted, freed = sweep(target, files, dirs)
    print("template-sweep: kept %d listed files, deleted %d entries (%d MB)" % (len(files), deleted, freed // 1024))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
