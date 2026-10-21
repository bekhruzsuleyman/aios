# Checklist

Acceptance checks per task with the command used and a link to evidence.

## T0: Host bootstrap, repo skeleton, anti-fake guard
- [x] `scripts/02-verify-tools.sh` exits 0. Evidence: [tests/evidence/T0/verify-tools.txt](tests/evidence/T0/verify-tools.txt)
- [x] `./check.sh` passes (fmt, clippy, tests, guard, guard self-test). Evidence: [tests/evidence/T0/check.txt](tests/evidence/T0/check.txt)
- [x] Guard proven: planting an HTML file, `package.json`, a webview crate, `FakeAgent`, `node`, a testkit file, or a `--kiosk` browser in a temp copy makes it fail. Command: `tests/guard/test-check-no-fake.sh`. Evidence: [tests/evidence/T0/guard-selftest.txt](tests/evidence/T0/guard-selftest.txt)

## T2: protocol, tracer, client
- [x] Round-trip tests for every message plus golden JSON fixtures. Command: `cargo test -p protocol` (6 unit + 4 golden). Evidence: [tests/evidence/T2/tests.txt](tests/evidence/T2/tests.txt)
- [x] Tracer test prints a three-stage timeline. Command: `cargo test -p tracer -- --nocapture three_stage_timeline`. Evidence: [tests/evidence/T2/timeline.txt](tests/evidence/T2/timeline.txt)
- [x] Client: request/response/event delivery, reconnect after the server goes away, approvals on the approval socket. Command: `cargo test -p client`
