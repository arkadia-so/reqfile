#!/usr/bin/env bash
set -euo pipefail

# Compile outside Reqfile's per-command timeouts, including on a cold runner.
cargo test --no-run --locked
cargo test --locked
# Use the binary just built, not a previously installed release.
./target/debug/reqfile check
./target/debug/reqfile test
