//! Wayland side of the shell: layer surfaces, input, the event loop, and
//! the bridge to aiosd through the `client` crate.

use crate::gfx;
use crate::text::Text;
use crate::ui::{self, Area, Command, Hit, Key, Link, Panel, Ui};
use client::{Client, ClientEvent};
use protocol::Call;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
    delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::calloop::{
        channel::{self, Channel},
        timer::{TimeoutAction, Timer},
        EventLoop, LoopHandle,
    },
    reexports::calloop_wayland_source::WaylandSource,
    reexports::client::{
        globals::registry_queue_init,
        protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
        Connection, QueueHandle,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long the top bar stays up after the pointer leaves it.
const BAR_HIDE_DELAY: Duration = Duration::from_millis(600);
use tiny_skia::Pixmap;
use tracer::Tracer;

/// Messages into the event loop from other threads.
pub enum Msg {
    Client(ClientEvent),
    TogglePalette,
    /// Open the palette, or re-create it (which takes keyboard focus back
    /// from an app window) if it is already open.
    ShowPalette,
    Wifi(crate::wifi::WifiState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Backdrop,
    Topbar,
    /// A one-pixel strip along the top edge, always above windows: the
    /// pointer touching it brings the top bar up.
    Edge,
    Palette,
    Status,
}

struct Surface {
    kind: Kind,
    layer: LayerSurface,
    width: u32,
    height: u32,
    configured: bool,
    dirty: bool,
    frame_pending: bool,
    areas: Vec<Area>,
}

pub struct Paths {
    pub agent: PathBuf,
    pub approve: PathBuf,
}

struct App {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: SlotPool,
    surfaces: Vec<Surface>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    modifiers: Modifiers,
    ui: Ui,
    text: Text,
    client: Client,
    backdrop: Option<(u32, u32, bool, Pixmap)>,
    /// Keypress waiting for its frame to be shown (keypress-to-frame trace).
    key_at: Option<Instant>,
    tracer: Tracer,
    last_minute: u64,
    start_req: Option<u64>,
    setup_req: Option<u64>,
    status_req: Option<u64>,
    recreate_palette: bool,
    wifi: std::sync::mpsc::Sender<crate::wifi::WifiCmd>,
    /// The pointer is on the top edge or the bar (or just left them).
    bar_hover: bool,
    bar_left_at: Option<Instant>,
    /// The top bar is in the Top layer (above windows), not Bottom.
    bar_on_top: bool,
}

pub fn run(paths: Paths, rx: Channel<Msg>, tx: channel::Sender<Msg>) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("wayland: {e}"))?;
    let (globals, queue) = registry_queue_init(&conn).map_err(|e| format!("wayland: {e}"))?;
    let qh: QueueHandle<App> = queue.handle();
    let mut event_loop: EventLoop<App> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), queue)
        .insert(handle.clone())
        .map_err(|e| format!("wayland source: {e}"))?;

    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|e| format!("wl_compositor: {e}"))?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| format!("layer shell: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("wl_shm: {e}"))?;
    let pool = SlotPool::new(4 * 1024 * 1024, &shm).map_err(|e| format!("shm pool: {e}"))?;

    let wifi = crate::wifi::spawn(tx.clone());
    let sink_tx = Mutex::new(tx);
    let client = Client::start(paths.agent, paths.approve, move |e| {
        if let Ok(tx) = sink_tx.lock() {
            let _ = tx.send(Msg::Client(e));
        }
    });

    let mut app = App {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        pool,
        surfaces: Vec::new(),
        keyboard: None,
        pointer: None,
        modifiers: Modifiers::default(),
        ui: Ui::new(),
        text: Text::new(),
        client,
        backdrop: None,
        key_at: None,
        tracer: Tracer::new(),
        last_minute: 0,
        start_req: None,
        setup_req: None,
        status_req: None,
        recreate_palette: false,
        wifi,
        bar_hover: false,
        bar_left_at: None,
        bar_on_top: false,
    };
    app.create(Kind::Backdrop, &qh);
    app.create(Kind::Topbar, &qh);
    app.create(Kind::Edge, &qh);
    app.create(Kind::Status, &qh);

    handle
        .insert_source(rx, |event, _, app: &mut App| {
            if let channel::Event::Msg(msg) = event {
                app.message(msg);
            }
        })
        .map_err(|e| format!("channel source: {e}"))?;
    handle
        .insert_source(Timer::from_duration(Duration::from_secs(1)), |_, _, app| {
            let minute = unix_now() / 60;
            if minute != app.last_minute {
                app.last_minute = minute;
                app.mark(Kind::Topbar);
            }
            TimeoutAction::ToDuration(Duration::from_secs(1))
        })
        .map_err(|e| format!("timer: {e}"))?;

    eprintln!("aios-shell: started");
    loop {
        event_loop
            .dispatch(Duration::from_millis(250), &mut app)
            .map_err(|e| format!("event loop: {e}"))?;
        app.flush(&qh, &handle);
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl App {
    fn create(&mut self, kind: Kind, qh: &QueueHandle<App>) {
        let surface = self.compositor.create_surface(qh);
        let (layer, ns) = match kind {
            Kind::Backdrop => (Layer::Background, "aios-backdrop"),
            // Below windows, so a window over it hides it; raised by
            // update_bar_layer().
            Kind::Topbar => (Layer::Bottom, "aios-topbar"),
            Kind::Edge => (Layer::Top, "aios-topbar-edge"),
            Kind::Palette => (Layer::Overlay, "aios-palette"),
            Kind::Status => (Layer::Top, "aios-status"),
        };
        let layer = self
            .layer_shell
            .create_layer_surface(qh, surface, layer, Some(ns), None);
        match kind {
            Kind::Backdrop => {
                layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
                layer.set_exclusive_zone(-1);
                layer.set_size(0, 0);
            }
            Kind::Topbar => {
                layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
                layer.set_size(0, ui::TOPBAR_HEIGHT);
                // No reserved space: windows may cover the bar.
                layer.set_exclusive_zone(0);
            }
            Kind::Edge => {
                layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
                layer.set_size(0, 1);
                layer.set_exclusive_zone(-1);
            }
            Kind::Palette => {
                // No anchor: centred by the compositor.
                layer.set_size(ui::PALETTE_WIDTH, 200);
                layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
            }
            Kind::Status => {
                layer.set_anchor(Anchor::BOTTOM);
                layer.set_size(ui::STATUS_WIDTH, ui::STATUS_HEIGHT);
                layer.set_margin(0, 0, 12, 0);
                // Never takes clicks: it is only text.
                if let Ok(region) = Region::new(&self.compositor) {
                    layer
                        .wl_surface()
                        .set_input_region(Some(region.wl_region()));
                }
            }
        }
        layer.commit();
        self.surfaces.push(Surface {
            kind,
            layer,
            width: 0,
            height: 0,
            configured: false,
            dirty: true,
            frame_pending: false,
            areas: Vec::new(),
        });
    }

    fn mark(&mut self, kind: Kind) {
        for s in &mut self.surfaces {
            if s.kind == kind {
                s.dirty = true;
            }
        }
    }

    fn mark_all(&mut self) {
        for s in &mut self.surfaces {
            s.dirty = true;
        }
    }

    fn message(&mut self, msg: Msg) {
        match msg {
            Msg::TogglePalette => {
                self.key_at = Some(Instant::now());
                self.ui.toggle_palette();
            }
            Msg::Wifi(state) => self.ui.set_wifi(state),
            Msg::ShowPalette => {
                if self.ui.panel == Panel::Hidden {
                    self.key_at = Some(Instant::now());
                    self.ui.toggle_palette();
                } else {
                    self.recreate_palette = true;
                }
            }
            Msg::Client(ClientEvent::Connected) => {
                self.ui.set_link(Link::Online { ai: None });
                self.status_req = self.client.send(Call::StatusGet {}).ok();
            }
            Msg::Client(ClientEvent::Disconnected(reason)) => {
                eprintln!("aios-shell: aiosd unavailable: {reason}");
                self.ui.set_link(Link::Offline);
            }
            Msg::Client(ClientEvent::Event(e)) => self.ui.apply(&e),
            Msg::Client(ClientEvent::Response(r)) => {
                let id = r.id.as_u64();
                if id.is_some() && id == self.start_req {
                    self.start_req = None;
                    let result = match (r.result, r.error) {
                        (Some(v), _) => v
                            .get("task_id")
                            .and_then(|t| t.as_str())
                            .map(str::to_string)
                            .ok_or_else(|| "aiosd sent no task id".to_string()),
                        (None, Some(e)) => Err(e.message),
                        (None, None) => Err("empty response".into()),
                    };
                    self.ui.started(result);
                } else if id.is_some() && id == self.setup_req {
                    self.setup_req = None;
                    let message = match (r.result, r.error) {
                        (Some(v), _) => v
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Saved.")
                            .to_string(),
                        (None, Some(e)) => e.message,
                        (None, None) => "No answer.".into(),
                    };
                    self.ui.setup_result(message);
                    self.status_req = self.client.send(Call::StatusGet {}).ok();
                } else if id.is_some() && id == self.status_req {
                    self.status_req = None;
                    let ai = r
                        .result
                        .and_then(|v| v.get("ai_configured").and_then(|b| b.as_bool()));
                    self.ui.set_link(Link::Online { ai });
                }
            }
        }
        self.mark_all();
    }

    fn commands(&mut self, cmds: Vec<Command>) {
        for cmd in cmds {
            match cmd {
                Command::Redraw => {}
                Command::Submit(text) => match self.client.send(Call::TaskStart { text }) {
                    Ok(id) => self.start_req = Some(id),
                    Err(e) => self.ui.started(Err(e.to_string())),
                },
                Command::Cancel(task_id) => {
                    let _ = self.client.send(Call::TaskCancel { task_id });
                }
                Command::Approve {
                    task_id,
                    approval_id,
                    allow,
                } => {
                    let _ = self.client.send(Call::ApprovalRespond {
                        task_id,
                        approval_id,
                        allow,
                    });
                }
                Command::WifiScan => {
                    let _ = self.wifi.send(crate::wifi::WifiCmd::Scan);
                }
                Command::WifiConnect {
                    path,
                    name,
                    passphrase,
                } => {
                    let _ = self.wifi.send(crate::wifi::WifiCmd::Connect {
                        path,
                        name,
                        passphrase,
                    });
                }
                Command::TestSetup {
                    endpoint,
                    model,
                    key,
                } => self.send_setup(endpoint, model, key, true),
                Command::SaveSetup {
                    endpoint,
                    model,
                    key,
                } => self.send_setup(endpoint, model, key, false),
            }
        }
        self.mark_all();
    }

    fn send_setup(&mut self, endpoint: String, model: String, api_key: String, test_only: bool) {
        match self.client.send(Call::SettingsAiSet {
            endpoint,
            model,
            api_key,
            test_only,
        }) {
            Ok(id) => self.setup_req = Some(id),
            Err(e) => self
                .ui
                .setup_result(format!("Could not reach the AiOS service: {e}")),
        }
    }

    /// Create or destroy the palette to match the UI state, then redraw
    /// every dirty, configured surface that is not waiting for a frame.
    fn flush(&mut self, qh: &QueueHandle<App>, _handle: &LoopHandle<'static, App>) {
        if std::mem::take(&mut self.recreate_palette) {
            self.surfaces.retain(|s| s.kind != Kind::Palette);
        }
        let want = self.ui.panel != Panel::Hidden;
        let have = self.surfaces.iter().any(|s| s.kind == Kind::Palette);
        if want && !have {
            self.create(Kind::Palette, qh);
        } else if !want && have {
            self.surfaces.retain(|s| s.kind != Kind::Palette);
            // Dropping the LayerSurface destroys it.
            if let Some(t) = self.key_at.take() {
                self.tracer
                    .record("key_to_frame", t.elapsed().as_nanos() as u64);
                log_key_to_frame(&self.tracer);
            }
        }
        self.update_bar_layer();
        for i in 0..self.surfaces.len() {
            let s = &self.surfaces[i];
            if s.configured && s.dirty && !s.frame_pending {
                self.draw(i, qh);
            }
        }
    }

    fn draw(&mut self, i: usize, qh: &QueueHandle<App>) {
        let kind = self.surfaces[i].kind;
        let (w, h) = (self.surfaces[i].width, self.surfaces[i].height);
        let (pixmap, areas) = match kind {
            Kind::Backdrop => {
                let dark = self.ui.dark;
                let fresh = !matches!(&self.backdrop, Some((bw, bh, bd, _)) if *bw == w && *bh == h && *bd == dark);
                if fresh {
                    let _t = self.tracer.stage("backdrop_render");
                    self.backdrop = gfx::backdrop(w, h, dark).map(|p| (w, h, dark, p));
                }
                match &self.backdrop {
                    Some((_, _, _, p)) => (p.clone(), Vec::new()),
                    None => return,
                }
            }
            Kind::Topbar => ui::draw_topbar(&self.ui, &mut self.text, w, unix_now()),
            Kind::Edge => match Pixmap::new(w.max(1), 1) {
                Some(p) => (p, Vec::new()),
                None => return,
            },
            Kind::Status => (ui::draw_status(&self.ui, &mut self.text), Vec::new()),
            Kind::Palette => {
                let (p, areas) = ui::draw_palette(&self.ui, &mut self.text);
                if p.height() != h {
                    // Ask for the new size; draw again after the configure.
                    let s = &mut self.surfaces[i];
                    s.layer.set_size(ui::PALETTE_WIDTH, p.height());
                    s.layer.commit();
                    s.configured = false;
                    s.dirty = true;
                    return;
                }
                (p, areas)
            }
        };
        let (pw, ph) = (pixmap.width(), pixmap.height());
        let stride = pw as i32 * 4;
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(pw as i32, ph as i32, stride, wl_shm::Format::Argb8888)
        else {
            eprintln!("aios-shell: no shm buffer");
            return;
        };
        gfx::to_argb(&pixmap, canvas);
        // The top bar surface is taller than the bar (room for the working
        // pill): only its drawn parts take clicks, the rest passes through.
        let input = if kind == Kind::Topbar {
            Region::new(&self.compositor).ok().inspect(|r| {
                for a in &areas {
                    r.add(a.x as i32, a.y as i32, a.w.ceil() as i32, a.h.ceil() as i32);
                }
            })
        } else {
            None
        };
        let s = &mut self.surfaces[i];
        if let Some(r) = &input {
            s.layer.wl_surface().set_input_region(Some(r.wl_region()));
        }
        let surface = s.layer.wl_surface();
        surface.damage_buffer(0, 0, pw as i32, ph as i32);
        surface.frame(qh, FrameCallbackData(surface.clone()));
        if buffer.attach_to(surface).is_err() {
            return;
        }
        s.layer.commit();
        s.areas = areas;
        s.dirty = false;
        s.frame_pending = true;
    }

    /// The top bar sits under windows and comes above them while the
    /// pointer is on the top edge or on the bar, while a task runs (its
    /// working pill and approvals hang under the bar) and while the palette
    /// is open.
    fn update_bar_layer(&mut self) {
        if let Some(t) = self.bar_left_at {
            if t.elapsed() >= BAR_HIDE_DELAY {
                self.bar_left_at = None;
                self.bar_hover = false;
            }
        }
        let top = self.bar_hover || self.ui.working() || self.ui.panel != Panel::Hidden;
        if top == self.bar_on_top {
            return;
        }
        self.bar_on_top = top;
        if let Some(s) = self.surfaces.iter().find(|s| s.kind == Kind::Topbar) {
            s.layer.set_layer(if top { Layer::Top } else { Layer::Bottom });
            s.layer.commit();
        }
    }

    fn pointer_crossing(&mut self, surface: &wl_surface::WlSurface, entered: bool) {
        let Some(kind) = self.index_of(surface).map(|i| self.surfaces[i].kind) else {
            return;
        };
        if !matches!(kind, Kind::Edge | Kind::Topbar) {
            return;
        }
        if entered {
            // Entering the bar itself only counts once it is up: under a
            // window the bar never gets the pointer anyway.
            if kind == Kind::Edge || self.bar_on_top {
                self.bar_hover = true;
                self.bar_left_at = None;
            }
        } else if self.bar_hover {
            self.bar_left_at = Some(Instant::now());
        }
    }

    fn index_of(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
    }
}

fn log_key_to_frame(tracer: &Tracer) {
    if let Some((_, ns)) = tracer
        .stages()
        .iter()
        .rev()
        .find(|(n, _)| n == "key_to_frame")
    {
        eprintln!("aios-shell: trace key_to_frame {:.2} ms", *ns as f64 / 1e6);
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(i) = self.index_of(surface) {
            self.surfaces[i].frame_pending = false;
            if self.surfaces[i].kind == Kind::Palette {
                if let Some(t) = self.key_at.take() {
                    self.tracer
                        .record("key_to_frame", t.elapsed().as_nanos() as u64);
                    log_key_to_frame(&self.tracer);
                }
            }
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.surfaces.retain(|s| &s.layer != layer);
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let Some(s) = self.surfaces.iter_mut().find(|s| &s.layer == layer) {
            let (w, h) = configure.new_size;
            s.width = if w == 0 { 1280 } else { w };
            s.height = if h == 0 { 800 } else { h };
            s.configured = true;
            s.dirty = true;
            s.frame_pending = false;
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
        if cap == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

fn to_key(event: &KeyEvent, mods: &Modifiers) -> Option<Key> {
    match event.keysym {
        Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
        Keysym::Escape => Some(Key::Escape),
        Keysym::BackSpace => Some(Key::Backspace),
        Keysym::Tab => Some(Key::Tab),
        Keysym::Up => Some(Key::Up),
        Keysym::Down => Some(Key::Down),
        _ => {
            let s = event.utf8.as_deref().unwrap_or("");
            if mods.alt {
                s.chars().next().map(|c| Key::Alt(c.to_ascii_lowercase()))
            } else if mods.ctrl || mods.logo || s.is_empty() || s.chars().any(char::is_control) {
                None
            } else {
                Some(Key::Text(s.to_string()))
            }
        }
    }
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if let Some(key) = to_key(&event, &self.modifiers) {
            self.key_at = Some(Instant::now());
            let cmds = self.ui.key(key);
            self.commands(cmds);
        }
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            match event.kind {
                PointerEventKind::Enter { .. } => self.pointer_crossing(&event.surface, true),
                PointerEventKind::Leave { .. } => self.pointer_crossing(&event.surface, false),
                _ => {}
            }
            let PointerEventKind::Press { .. } = event.kind else {
                continue;
            };
            let Some(i) = self.index_of(&event.surface) else {
                continue;
            };
            let hit: Option<Hit> =
                ui::hit_test(&self.surfaces[i].areas, event.position.0, event.position.1);
            if let Some(hit) = hit {
                self.key_at = Some(Instant::now());
                let cmds = self.ui.click(hit);
                self.commands(cmds);
            }
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_dispatch2!(App);
