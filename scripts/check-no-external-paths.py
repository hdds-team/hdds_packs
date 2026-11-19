#!/usr/bin/env python3
"""Refuse a path that escapes this repository.

Every demo here is meant to build from a clone and nothing else, so a path
pointing outside the clone only works on the machine it was written on. The
check resolves each path against the repository root rather than matching how it
is spelled, because the same defect appears both as an absolute path and as a
chain of "..".

Two kinds of path are inspected:

* a Cargo dependency declared with `path = "..."`, in any `.toml`. A dependency
  on a sibling crate inside the repository is normal: each demo depends on its
  own generated types crate that way.
* a relative path written in a `Makefile` or a shell script. An escaping one is
  worse than a broken build: `realpath` on a missing directory expands to the
  empty string, which turns `rm -rf $(VAR)/target` into a path at the root of
  the filesystem.

Usage: python3 scripts/check-no-external-paths.py [root]
Exit status: 0 when every path stays inside, 1 otherwise.
"""

import os
import re
import sys

DEP_PATH = re.compile(r'^\s*(?P<name>[\w-]+)\s*=\s*\{[^}]*\bpath\s*=\s*"(?P<path>[^"]+)"', re.M)
DOTDOT = re.compile(r'(?<![\w./])(?:\.\./)+[\w./-]*')
SKIP_DIRS = {".git", "target", "node_modules", "dist", ".embuild", "build"}
SCRIPT_NAMES = ("Makefile", "makefile", "GNUmakefile")


def walk(root):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            yield os.path.join(dirpath, name)


def escapes(root, base, candidate):
    """True when candidate, resolved from base, lands outside root."""
    target = os.path.normpath(os.path.join(base, candidate))
    return target != root and not target.startswith(root + os.sep), target


def main(argv):
    root = os.path.abspath(argv[1] if len(argv) > 1 else ".")
    inside = outside = 0

    for path in sorted(walk(root)):
        name = os.path.basename(path)
        is_manifest = name.endswith(".toml")
        is_script = name in SCRIPT_NAMES or name.endswith(".sh") or name.endswith(".mk")
        if not (is_manifest or is_script):
            continue
        try:
            text = open(path, encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        base = os.path.dirname(os.path.abspath(path))
        rel = os.path.relpath(path, root)

        if is_manifest:
            for match in DEP_PATH.finditer(text):
                out, target = escapes(root, base, match.group("path"))
                if out:
                    outside += 1
                    print('FAIL %s: dependency %s has path "%s", which resolves '
                          "outside the repository (%s)"
                          % (rel, match.group("name"), match.group("path"), target))
                else:
                    inside += 1

        if is_script:
            for lineno, line in enumerate(text.splitlines(), 1):
                if line.lstrip().startswith("#"):
                    continue
                for candidate in set(DOTDOT.findall(line)):
                    out, target = escapes(root, base, candidate)
                    if out:
                        outside += 1
                        print('FAIL %s:%d: the path "%s" resolves outside the '
                              "repository (%s)" % (rel, lineno, candidate, target))
                    else:
                        inside += 1

    if outside:
        print("check-no-external-paths: FAILED (%d outside, %d inside)"
              % (outside, inside))
        return 1
    print("check-no-external-paths: %d paths checked, all inside the repository"
          % inside)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
