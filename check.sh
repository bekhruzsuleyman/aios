#!/usr/bin/env bash
# Quality gate run before every commit (plan section 1.8).
set -euo pipefail
cd "$(dirname "$0")"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/check-no-fake.sh
tests/guard/test-check-no-fake.sh
