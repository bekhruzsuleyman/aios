# Decisions

Assumptions made while implementing the 0.1 plan. Each entry names the task it came from.

## T0
- **Crate edition.** `cargo new` now defaults to edition 2024; every crate is pinned to 2021 via `[workspace.package]` as the plan requires.
- **No unwrap/expect.** Enforced with workspace clippy lints (`unwrap_used`, `expect_used` = deny), relaxed in tests through `clippy.toml`. `main` uses `?`, so it needs no exception.
- **check.sh** runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. `--all-targets` so tests are linted too.
- **Extra config keys.** `agent = "fake" | "real"` (T5 asks for it), `paths.shell` (where aiosd serves the shell from) and `[shell] listen` (the loopback WebSocket bridge, T3) were added to `config/aios.toml`.
