#!/usr/bin/env bash
# Anti-fake guard (plan section 5). Fails on any sign of a web UI, webview,
# scripted/fake agent, or test-only code in the shipped image.
#
#   scripts/check-no-fake.sh [--repo DIR] [--rootfs DIR]
#
# --repo defaults to this checkout. --rootfs additionally scans a built
# image root filesystem (used by the image build).
set -uo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
ROOTFS=""
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) REPO="$2"; shift 2 ;;
    --rootfs) ROOTFS="$2"; shift 2 ;;
    *) echo "usage: $0 [--repo DIR] [--rootfs DIR]" >&2; exit 2 ;;
  esac
done
fail=0
bad() { echo "check-no-fake: $*" >&2; fail=1; }

cd "$REPO" || exit 2
prune=(-path ./tests -o -path ./third_party -o -path ./target -o -path ./build -o -path ./logs -o -path ./.git)

# 1. No web UI files outside tests/ and third_party/.
while IFS= read -r f; do
  bad "web file outside tests/: $f"
done < <(find . \( "${prune[@]}" \) -prune -o -type f \( -name '*.html' -o -name '*.htm' -o -name '*.js' -o -name '*.mjs' -o -name '*.cjs' -o -name '*.css' -o -name 'package.json' \) -print)

# 2. No webview / web-runtime crates in non-test crates.
WEB_CRATES='tauri|wry|webkit2gtk|webkit6|webview|web-view|electron|cef|servo'
for toml in crates/*/Cargo.toml; do
  [ -f "$toml" ] || continue
  if grep -Eiq "^[[:space:]]*(${WEB_CRATES})([-_a-z0-9]*)?[[:space:]]*=" "$toml"; then
    bad "web runtime dependency in $toml"
  fi
done
if [ -f Cargo.lock ] && grep -Eq "^name = \"(${WEB_CRATES})([-_a-z0-9]*)?\"" Cargo.lock; then
  bad "web runtime crate in Cargo.lock: $(grep -Eo "^name = \"(${WEB_CRATES})[-_a-z0-9]*\"" Cargo.lock | head -3 | tr '\n' ' ')"
fi

# 3. No fake/scripted agents in shipped sources and image config.
FAKE_WORDS='FakeAgent|MockAgent|scripted|demo_mode'
for d in crates config image; do
  [ -d "$d" ] || continue
  while IFS= read -r hit; do
    bad "forbidden word in shipped source: $hit"
  done < <(grep -rnE "$FAKE_WORDS" "$d" 2>/dev/null)
done

# 4. Built image: no test kit, no node/electron, no kiosk browser.
if [ -n "$ROOTFS" ]; then
  [ -d "$ROOTFS" ] || { echo "check-no-fake: rootfs $ROOTFS not found" >&2; exit 2; }
  while IFS= read -r f; do
    bad "test-only file in image: $f"
  done < <(find "$ROOTFS" \( -path "$ROOTFS/proc" -o -path "$ROOTFS/sys" \) -prune -o \( -iname '*testkit*' -o -iname 'mock-ai*' -o -iname 'mock_ai*' \) -print 2>/dev/null)
  for exe in node nodejs electron; do
    for dir in usr/bin usr/local/bin bin usr/lib/node_modules; do
      [ -e "$ROOTFS/$dir/$exe" ] && bad "forbidden runtime in image: /$dir/$exe"
    done
  done
  for d in etc usr/lib/systemd/system etc/systemd; do
    [ -d "$ROOTFS/$d" ] || continue
    while IFS= read -r hit; do
      bad "kiosk browser configured in image: $hit"
    done < <(grep -rlE -- '--kiosk' "$ROOTFS/$d" 2>/dev/null)
  done
fi

if [ $fail -ne 0 ]; then
  echo "check-no-fake: FAILED" >&2
  exit 1
fi
echo "check-no-fake: OK"
