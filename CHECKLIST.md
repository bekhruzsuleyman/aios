# Checklist

Each task's acceptance checks, ticked when they pass, with the command used.

## T0: Workspace and dev loop
- [x] Layout from plan section 3 created; workspace with five empty crates.
- [x] `./check.sh` passes on the empty workspace. Command: `./check.sh`

## T1: protocol and tracer
- [x] Round-trip tests for every event, request and response. Command: `cargo test -p protocol` (6 passed)
- [x] Tracer test prints a three-stage timeline. Command: `cargo test -p tracer -- --nocapture three_stage_timeline`

## T2: aiosd skeleton and fake agent
- [x] Test client connects, starts a task and receives the full ordered event sequence (seq 0..n, plan, 4× started/done, result; a second client sees the identical stream). Command: `cargo test -p aiosd --test fake_agent full_ordered_event_sequence`
- [x] `task.cancel` produces `cancelled`, with nothing after it. Command: `cargo test -p aiosd --test fake_agent cancel_produces_cancelled`
