# AiOS compositor plan

Status: proposal, waiting for Bekhruz's review. Nothing here is built yet.

## Why

The design (see `docs/design/`) needs things labwc cannot do: real frosted
glass behind the palette and top bar, our own window chrome (three dots,
small centred uppercase title, soft shadow, rounded corners), and calm
open/close animations. Owning the compositor also makes it the trust root
for the screen and keyboard, which matters for approvals later.

## What it is

A new crate, `crates/compositor`, building one binary, `aios-compositor`,
on [smithay](https://github.com/Smithay/smithay) (the Rust Wayland
compositor library that COSMIC and niri are built on). It replaces labwc in
`aios-session.service`. `aios-shell` does not change: it stays a separate
layer-shell client, so everything in T4 to T7 keeps working.

```
 aios-session.service
   └─ aios-compositor          (new: DRM/KMS, input, windows, effects)
        ├─ aios-shell           (unchanged: top bar, palette, toast; layer-shell)
        └─ apps in bubblewrap   (Firefox, Mousepad, foot; xdg-shell)
```

## Decisions (defaults; say if you want different)

| Topic | Default | Why |
|---|---|---|
| Window layout | Floating, new windows centred, like the mockup | The mockup is a floating desktop, not tiles |
| Backend | DRM/KMS + GBM + libinput through libseat (real hardware and QEMU); winit backend for development in a window | Same code path on the stick and in the VM |
| Renderer | OpenGL ES 2 (smithay `GlesRenderer`) | Needed for blur and rounded-corner shaders; works on Intel, AMD, NVIDIA (nouveau) and on Mesa's software renderer in QEMU |
| Blur | Dual-Kawase blur, only behind layer surfaces that ask for it (palette, top bar, toast) | Cheap enough for integrated GPUs; nothing else is blurred |
| Window chrome | Server-side, drawn by the compositor; apps are told so with `xdg-decoration` | One look for every app (Firefox honours it) |
| Xwayland | Not in the first version | Firefox, Mousepad and foot are native Wayland |
| Protocols | xdg-shell, xdg-decoration, wlr-layer-shell, data-device (clipboard), xdg-activation, viewporter, fractional-scale, presentation-time, cursor-shape | What the shell and the three apps use |
| Fallback | labwc stays in the image; `aios.wm=labwc` on the boot command line switches back | A broken compositor must never leave you with a black screen |

## Phases

Each phase ends with VM screenshots and tests, like T3 to T8.

1. **C0 skeleton.** Outputs, input, xdg-shell and layer-shell; the shell, foot and Firefox run; Super+Space, Alt+Tab, close, move and resize. Parity check against labwc in the existing flow test.
2. **C1 chrome.** Our title bars (three dots, centred uppercase title), rounded corners, soft shadows, focus states, light and dark.
3. **C2 glass.** Blur behind the palette, top bar and toast; tuned against the mockup.
4. **C3 motion.** Window open/close (fade plus slight scale, about 180 ms), palette fade, the working pill sliding in. Frame-time budget logged with the tracer.
5. **C4 switch.** All T4 to T8 tests pass on `aios-compositor`; it becomes the default and labwc the fallback.

## Risks, honestly

- This is weeks of work, not days. C0 alone is the bulk of a small compositor.
- In QEMU without a GPU, blur runs on the CPU (Mesa llvmpipe) and will be slow; real numbers need KVM with virtio-gpu, or the real machine.
- A compositor crash takes the whole desktop down. The labwc fallback and the session's automatic restart cover that while it matures.
- NVIDIA's proprietary driver is out of scope for 0.x (nouveau only).
