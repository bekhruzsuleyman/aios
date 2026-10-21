# Decisions

Assumptions made while implementing the 0.1 plan. Each entry names the task it came from.

## T0
- **Crate edition.** `cargo new` now defaults to edition 2024; every crate is pinned to 2021 via `[workspace.package]` as the plan requires.
- **No unwrap/expect.** Enforced with workspace clippy lints (`unwrap_used`, `expect_used` = deny), relaxed in tests through `clippy.toml`. `main` uses `?`, so it needs no exception.
- **check.sh** runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. `--all-targets` so tests are linted too.
- **Extra config keys.** `agent = "fake" | "real"` (T5 asks for it), `paths.shell` (where aiosd serves the shell from) and `[shell] listen` (the loopback WebSocket bridge, T3) were added to `config/aios.toml`.

## T1
- **Requests vs. envelope fields.** Section 5 says every message has `ts_ns`, `task_id`, `seq`. That holds for events. Requests are plain JSON-RPC 2.0 (`jsonrpc`, `id`, `method`, `params`) because `task.start` has no task yet and the JSON-RPC id already orders responses.
- **One connection, two kinds of lines.** Responses and events share the client connection, one JSON object per line. Clients tell them apart by the `jsonrpc` key (`protocol::ServerMsg` is an untagged enum).
- **Event tag.** Events are internally tagged with `"type": "<snake_case name>"`, flattened next to `ts_ns`/`task_id`/`seq`.
- **Monotonic clock.** `tracer::now_ns()` is nanoseconds since a process-wide `Instant` epoch (monotonic, no `unsafe`/libc). Timestamps are comparable within one aiosd process, not across processes.
- **Unknown methods are rejected by deserialisation.** `Call` is a closed enum, so e.g. `shell.exec` cannot even be parsed as a request.

## T2
- **Event fan-out.** All clients receive the events of all tasks (a `tokio::sync::broadcast` channel); clients filter by `task_id`. Lagging clients drop events with a log line rather than blocking the daemon.
- **Response before events.** A `task.start` task waits on a gate that is opened only after the response is queued, and the writer prefers responses (`biased` select), so a client always learns its `task_id` before the first event.
- **Cancellation** aborts the task's tokio future (which drops any in-flight I/O), then emits `cancelled` and a `trace { stage: "cancel" }` with the time spent cancelling. After a terminal event (`result`, `error`, `cancelled`) the task's emitter is closed, so nothing (e.g. traces from stage guards dropped by the abort) can follow it.
- **FakeAgent pacing.** Steps take 2–4 × a base unit (100 ms in the daemon, 5 ms in tests); planning takes 3 units.
- **Per-task timeline** is printed to stderr when a task ends, and every stage is also streamed as a `trace` event.
- **Dev config.** `config/dev.toml` uses `$XDG_RUNTIME_DIR` / `$HOME` paths; aiosd expands a leading `$XDG_RUNTIME_DIR` or `$HOME` in path settings. Default config path is `/etc/aios/aios.toml`, overridden with `--config`.
