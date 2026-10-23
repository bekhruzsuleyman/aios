# Files fetched outside the build

The build host's network proxy blocks a few sources, so these files are
downloaded on another machine (any Linux shell with normal internet, e.g.
WSL) into `third_party/downloads/` and copied over. `scripts/20-rootfs.sh`
checks them before use.

| File | Source | Check |
|---|---|---|
| `firefox_<ver>_amd64_<md5>.deb` | Mozilla's APT repo: `https://packages.mozilla.org/apt`, newest `Package: firefox` in `dists/mozilla/main/binary-amd64/Packages` | SHA256 against `firefox.Packages.txt` (the Packages entry) |
| `firefox.Packages.txt` | that Packages entry, verbatim | `SHA256SUMS` |
| `repo-signing-key.gpg` | `https://packages.mozilla.org/apt/repo-signing-key.gpg` | for checking `InRelease` with `gpgv` |
| `InstrumentSerif-Italic.ttf`, `InstrumentSerif-Regular.ttf`, `OFL.txt` | `https://github.com/google/fonts/tree/main/ofl/instrumentserif` | `SHA256SUMS` |

Write `SHA256SUMS` with `sha256sum * > SHA256SUMS` after downloading.
