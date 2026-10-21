#!/usr/bin/env bash
# Resolve the newest longterm kernel and fetch its source into third_party/.
# Primary source: kernel.org tarball with sha256 check (plan section 6.1).
# Fallback (kernel.org unreachable): the stable tree mirror on GitHub
# (github.com/gregkh/linux) at the release tag, shallow clone; the tag's
# commit id is recorded in third_party/KERNEL_SOURCE.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p third_party
cd third_party

retry() { local n=0; until "$@"; do n=$((n+1)); [ $n -ge 3 ] && return 1; sleep $((n*5)); done; }

KVER=""
if retry curl -fsSL -m 30 https://www.kernel.org/releases.json -o releases.json; then
  KVER=$(python3 - <<'PY'
import json
r = json.load(open("releases.json"))["releases"]
lts = [x for x in r if x["moniker"] == "longterm" and not x.get("iseol")]
lts.sort(key=lambda x: tuple(int(p) for p in x["version"].split(".") if p.isdigit()), reverse=True)
print((lts or [x for x in r if x["moniker"] == "stable"])[0]["version"])
PY
)
fi
# kernel.org unreachable: LTS series pinned in DECISIONS.md, newest point
# release taken from the mirror's tags.
LTS_SERIES=${LTS_SERIES:-6.18}
MIRROR=https://github.com/gregkh/linux
if [ -z "$KVER" ]; then
  KVER=$(git ls-remote --tags "$MIRROR" "refs/tags/v${LTS_SERIES}.*" | grep -v '\^{}' \
    | sed 's|.*refs/tags/v||' | sort -V | tail -1)
fi
[ -n "$KVER" ] || { echo "could not resolve kernel version" >&2; exit 1; }
echo "kernel $KVER"

if [ -d "linux-$KVER" ]; then
  echo "linux-$KVER already present"
else
  MAJOR=${KVER%%.*}
  ok=0
  for BASE in https://cdn.kernel.org/pub/linux/kernel/v${MAJOR}.x https://mirrors.edge.kernel.org/pub/linux/kernel/v${MAJOR}.x; do
    if retry curl -fsSL -m 600 -O "$BASE/linux-$KVER.tar.xz" && retry curl -fsSL -O "$BASE/sha256sums.asc"; then
      grep " linux-$KVER.tar.xz" sha256sums.asc | sha256sum -c -
      tar -xf "linux-$KVER.tar.xz"
      echo "tarball $BASE/linux-$KVER.tar.xz" > KERNEL_SOURCE
      ok=1
      break
    fi
  done
  if [ $ok -eq 0 ]; then
    retry git clone -q --depth 1 --branch "v$KVER" "$MIRROR" "linux-$KVER"
    commit=$(git -C "linux-$KVER" rev-parse HEAD)
    echo "git $MIRROR tag v$KVER commit $commit" > KERNEL_SOURCE
  fi
fi
echo "$KVER" > KERNEL_VERSION
cat KERNEL_SOURCE
