#!/usr/bin/env bash
# Proves scripts/check-no-fake.sh works: it passes on a clean copy of the
# repo and fails when each kind of forbidden thing is planted in a copy.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
GUARD="$REPO/scripts/check-no-fake.sh"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

fresh() {
  rm -rf "$WORK/repo"
  mkdir -p "$WORK/repo"
  (cd "$REPO" && tar --exclude=./target --exclude=./build --exclude=./third_party --exclude=./logs --exclude=./.git -cf - .) | tar -xf - -C "$WORK/repo"
}

expect_fail() {
  local what=$1; shift
  if "$GUARD" "$@" >/dev/null 2>&1; then
    echo "FAIL: guard did not catch: $what"; exit 1
  fi
  echo "ok: caught $what"
}

fresh
"$GUARD" --repo "$WORK/repo" >/dev/null || { echo "FAIL: guard rejects clean repo"; exit 1; }
echo "ok: clean repo passes"

fresh; mkdir -p "$WORK/repo/shell"; echo '<html></html>' > "$WORK/repo/shell/index.html"
expect_fail "html file" --repo "$WORK/repo"

fresh; echo '{}' > "$WORK/repo/package.json"
expect_fail "package.json" --repo "$WORK/repo"

fresh; mkdir -p "$WORK/repo/crates/x/src"; printf '[package]\nname="x"\n[dependencies]\nwry = "0.40"\n' > "$WORK/repo/crates/x/Cargo.toml"
expect_fail "webview crate" --repo "$WORK/repo"

fresh; mkdir -p "$WORK/repo/crates/y/src"; echo 'struct FakeAgent;' > "$WORK/repo/crates/y/src/lib.rs"
expect_fail "FakeAgent in shipped source" --repo "$WORK/repo"

fresh; mkdir -p "$WORK/rootfs/usr/bin" "$WORK/rootfs/etc"
"$GUARD" --repo "$WORK/repo" --rootfs "$WORK/rootfs" >/dev/null || { echo "FAIL: guard rejects clean rootfs"; exit 1; }
touch "$WORK/rootfs/usr/bin/node"
expect_fail "node in image" --repo "$WORK/repo" --rootfs "$WORK/rootfs"
rm "$WORK/rootfs/usr/bin/node"; mkdir -p "$WORK/rootfs/usr/libexec/aios-testkit"
expect_fail "testkit in image" --repo "$WORK/repo" --rootfs "$WORK/rootfs"
rm -rf "$WORK/rootfs/usr/libexec"; echo 'ExecStart=/usr/bin/firefox --kiosk http://x' > "$WORK/rootfs/etc/kiosk.service"
expect_fail "kiosk browser in image" --repo "$WORK/repo" --rootfs "$WORK/rootfs"
echo "guard self-test passed"
