#!/usr/bin/env bash
# The image's own built-in AI at native speed: runs build/rootfs's
# /usr/lib/aios/aios-llm (the shipped llama-server, model and flags) in a
# chroot on the host, points the host's aiosd (--dev) at it through the
# image's /etc/aios/aios.toml, and runs real requests. For hosts without
# KVM, where tests/boot/desktop_ai_test.py cannot wait for a plan. Root only.
set -euo pipefail
cd "$(dirname "$0")/../.."
R=build/rootfs
EV=tests/evidence/AI
mkdir -p "$EV"
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/tmp/aios-native-xdg}
mkdir -p "$XDG_RUNTIME_DIR"
mount --bind /proc $R/proc; mount --bind /dev $R/dev
cleanup() { kill "$LLM" "$D" 2>/dev/null || true; sleep 1; umount $R/proc $R/dev || true; }
chroot $R /usr/lib/aios/aios-llm > "$EV/native-llm.log" 2>&1 & LLM=$!
trap cleanup EXIT
for _ in $(seq 120); do curl -sf 127.0.0.1:8180/health >/dev/null && break; sleep 1; done
rm -rf build/dev-data
target/release/aiosd --dev --config $R/etc/aios/aios.toml > "$EV/native-aiosd.log" 2>&1 & D=$!
sleep 2
{
  for q in "save a note titled Shopping with milk and eggs" \
           "what is the capital of Japan?" \
           "find reviews of the framework laptop 13"; do
    echo "== $q"
    target/release/aios-testkit-client run "$XDG_RUNTIME_DIR/aios/agent.sock" "$q" --timeout-s 300 2>&1 \
      | grep -E '"plan_created"|"type":"result"|"type":"error"|SUMMARY'
  done
  echo "== notes"; grep -ril milk build/dev-data/notes
} | tee "$EV/native-run.txt"
grep -q '"action":"notes.create"' "$EV/native-run.txt"
grep -q 'Tokyo' "$EV/native-run.txt"
grep -q '"action":"web.search"' "$EV/native-run.txt"
echo PASS
