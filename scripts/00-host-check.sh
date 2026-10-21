#!/usr/bin/env bash
# Record host capabilities in logs/host.txt (plan section 4.1).
set -uo pipefail
cd "$(dirname "$0")/.."
mkdir -p logs
out=logs/host.txt
{
  echo "date: $(date -u +%FT%TZ)"
  echo "os: $(. /etc/os-release && echo "$PRETTY_NAME")"
  echo "kernel: $(uname -r)"
  echo "cpus: $(nproc)"
  echo "ram: $(free -g | awk '/Mem:/{print $2" GiB"}')"
  echo "disk_free: $(df -BG --output=avail . | tail -1 | tr -d ' ')"
  if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then echo "kvm: yes"; else echo "kvm: no (QEMU uses -accel tcg; timings are emulated)"; fi
  if [ "$(id -u)" -eq 0 ]; then echo "root: yes"; elif sudo -n true 2>/dev/null; then echo "sudo: passwordless"; else echo "sudo: no"; fi
  for t in docker podman; do command -v $t >/dev/null && echo "$t: $(command -v $t)"; done
  for url in https://www.kernel.org https://cdn.kernel.org https://github.com/gregkh/linux https://index.crates.io/config.json https://deb.debian.org/debian/ https://archive.ubuntu.com/ubuntu/; do
    code=$(curl -sS -o /dev/null -m 15 -w '%{http_code}' "$url" 2>/dev/null); echo "reach $url: ${code:-000}"
  done
} > "$out"
cat "$out"
