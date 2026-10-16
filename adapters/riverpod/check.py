#!/usr/bin/env python3
"""Verify Riverpod compatibility from Dart's resolved dependency graph."""

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
        command = binding.get("command")
        require(isinstance(command, list) and command and isinstance(command[0], str), "Missing Dart command argv")
        require(Path(command[0]).name in ("dart", "dart.exe"), "Riverpod command must start with the Dart executable")
        code, out, _ = execute([command[0], "pub", "deps", "--json"], cwd, os.environ.copy(), 30)
        require(code == 0, "Cannot read the project's resolved Dart dependency graph")
        graph = load_json(out)
        packages = graph.get("packages") if isinstance(graph, dict) else None
        require(isinstance(packages, list), "Dart dependency graph has no packages")
        matches = [p for p in packages if isinstance(p, dict) and p.get("name") == "riverpod"]
        require(len(matches) == 1 and re.fullmatch(r"3\.\d+\.\d+", str(matches[0].get("version"))),
                "Riverpod adapter supports resolved stable major 3 only")
    except (EvidenceError, OSError, ValueError) as error:
        print(f"{identity}: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
