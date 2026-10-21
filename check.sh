#!/usr/bin/env bash
# Dev loop / CI gate: formatting, lints, tests. Fails on the first error.
set -euo pipefail
cd "$(dirname "$0")"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
