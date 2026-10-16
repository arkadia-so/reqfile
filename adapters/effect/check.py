#!/usr/bin/env python3
"""Verify Effect compatibility from the checked project's resolved package."""

import argparse
import os
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from scenarios import EvidenceError, execute, inside, load_json, load_project, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--requirement", required=True)
    identity = parser.parse_args().requirement
    try:
        root = Path.cwd().resolve()
        binding, _, _ = load_project(identity, root)
        cwd = inside(root, binding.get("cwd", "."), "cwd")
        script = "const p=require.resolve('effect/package.json',{paths:[process.cwd()]});process.stdout.write(require('fs').readFileSync(p,'utf8'))"
        code, out, _ = execute(["node", "-e", script], cwd, os.environ.copy(), 30)
        require(code == 0, "Cannot resolve the project's installed Effect package")
        package = load_json(out)
        version = package.get("version") if isinstance(package, dict) else None
        require(isinstance(version, str) and re.fullmatch(r"4\.\d+\.\d+", version),
                "Effect adapter supports installed stable major 4 only")
    except (EvidenceError, OSError, ValueError) as error:
        print(f"{identity}: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
