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
| `models/Qwen3-0.6B-Q4_K_M.gguf`, `models/Qwen3-1.7B-Q4_K_M.gguf` | Hugging Face `unsloth/Qwen3-0.6B-GGUF` and `unsloth/Qwen3-1.7B-GGUF` (the `Qwen/` repos publish only Q8_0); Apache-2.0, `LICENSE.Qwen3-upstream.txt` from `Qwen/Qwen3-0.6B-GGUF` | `models/SHA256SUMS` (the LFS sha256 Hugging Face shows) |
| `models/DEFAULT` | one line: the file name of the model to ship as the built-in AI | |

Write `SHA256SUMS` with `sha256sum * > SHA256SUMS` after downloading.

Built from source by our scripts (not downloaded by hand): llama.cpp at a
fixed tag (`scripts/15-llama.sh`, cloned from GitHub) and labwc 0.7.1 from
the Ubuntu archive's source tarball, checked by sha256
(`scripts/16-labwc.sh`).
