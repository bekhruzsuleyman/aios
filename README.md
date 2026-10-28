# AiOS

An AI-first operating system on the Linux kernel: our own kernel build, a read-only image, a native Rust Wayland desktop shell, and an agent daemon whose every action passes a permission broker.

Status: 0.1 complete in QEMU; the physical-hardware boot is pending (see `REPORT.md`). See `PLAN.md` (the plan), `STATE.md` (where the build is), `CHECKLIST.md` (what is proven, with evidence) and `DECISIONS.md`.

## Building
```
scripts/00-host-check.sh      # what this host can do
scripts/01-install-tools.sh   # host packages, mkosi, Rust
scripts/02-verify-tools.sh
scripts/10-kernel-fetch.sh && scripts/11-kernel-config.sh && scripts/13-kernel-build.sh
./check.sh                    # fmt, clippy, tests, anti-fake guard
scripts/20-rootfs.sh && scripts/21-disk.sh   # build/aios.img (and build/aios-test.img)
scripts/22-run-qemu.sh build/aios.img        # boot it (UEFI, QEMU)
```

## Using it
Press **Super+Space** (the Windows key and Space; **Alt+Space** also works, which helps inside a VM where Windows keeps the Windows key) to open the palette and type a request. These five commands always work, even with no AI connected:

| Command | What happens |
|---|---|
| `open browser` / `open editor` / `open terminal` | asks you first, then starts the app in a sandbox |
| `search <words>` | opens the browser on a DuckDuckGo search |
| `play <something>` | opens the browser on a YouTube search |
| `note: <text>` | saves a note in `/var/lib/aios/notes` |
| `read <file>` | shows a file from `/var/lib/aios/user` |

Keys in the palette: **Enter** to send, **Alt+A** / **Alt+D** to allow or deny an action, **Alt+C** to stop, **Alt+S** for AI settings, **Esc** to close.

## Connecting an AI
AiOS talks to any **OpenAI-compatible** chat completions endpoint that supports tool calls and streaming. Open the palette, press **Alt+S** (or click the AI status chip in the top bar), fill in the three fields and press **Save** (or **Test** to only try it). The test sends one tiny real request and shows the real answer or the real error.

| Field | Example |
|---|---|
| Endpoint | `https://api.openai.com/v1`, or just `192.168.1.50:8000` for a GPU box on your network (`http://` and `/v1` are added) |
| Model | the model name the server expects, e.g. `gpt-4o-mini` or `qwen2.5-7b-instruct` |
| API key | the key, or empty for a local server without auth |

Examples:
- **Cloud API:** endpoint `https://api.openai.com/v1` (or any compatible provider's base URL), your model name and key.
- **A GPU box by IP running vLLM:** `vllm serve Qwen/Qwen2.5-7B-Instruct --enable-auto-tool-choice --tool-call-parser hermes --port 8000`, then endpoint `192.168.1.50:8000`, model `Qwen/Qwen2.5-7B-Instruct`, empty key.
- **llama.cpp:** `llama-server -m model.gguf --jinja --host 0.0.0.0 --port 8080`, then endpoint `IP:8080`, model as reported by `/v1/models`, empty key.

Settings are stored on the data partition: `/var/lib/aios/ai.toml` (endpoint and model) and `/var/lib/aios/ai.key` (mode 0600, readable only by the `aiosd` service). Without an AI the shell says "AI not connected" and only the five commands work; AiOS never makes up a plan.

What the model can do: it only chooses among the five actions above. Every action goes through the permission broker (risk policy in `PLAN.md` section 8, audit log in `/var/lib/aios/audit.log`). File contents and search results go back to the model only inside `<untrusted_data>` blocks, and after the model has seen such data it is asked for a final answer *without* tools, so nothing it reads can trigger an action.
