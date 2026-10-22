//! Shell state, input handling and drawing. Pure: no Wayland here, so it is
//! unit-tested on any host.

use crate::gfx::{self, Theme};
use crate::text::{Text, TextStyle};
use crate::wifi::WifiState;
use protocol::{Event, EventMsg, Risk};
use std::collections::VecDeque;
use tiny_skia::{Color, Pixmap};

pub const TOPBAR_HEIGHT: u32 = 56;
pub const PALETTE_WIDTH: u32 = 720;
pub const STATUS_WIDTH: u32 = 900;
pub const STATUS_HEIGHT: u32 = 48;
/// Transparent margin around the palette card for its shadow.
const MARGIN: f32 = 28.0;
const PAD: f32 = 28.0;

#[derive(Debug, Clone, PartialEq)]
pub enum StepState {
    Pending,
    Running,
    Done,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub state: StepState,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Idle,
    Starting,
    Working,
    Done,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub task_id: String,
    pub approval_id: String,
    pub action: String,
    pub args: String,
    pub risk: Risk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Hidden,
    Ask,
    Setup,
    Wifi,
}

/// The Wi-Fi picker: iwd's state plus what the user is doing with it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WifiView {
    pub state: WifiState,
    pub selected: usize,
    /// Asking for the selected network's password.
    pub asking: bool,
    pub password: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetupForm {
    pub endpoint: String,
    pub model: String,
    pub key: String,
    pub focus: usize,
    pub message: String,
    pub busy: bool,
}

impl SetupForm {
    fn field_mut(&mut self) -> &mut String {
        match self.focus {
            0 => &mut self.endpoint,
            1 => &mut self.model,
            _ => &mut self.key,
        }
    }
}

/// Connection to aiosd and what it reported about the AI backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Offline,
    Online { ai: Option<bool> },
}

/// Things the UI asks the app to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Submit(String),
    Cancel(String),
    Approve {
        task_id: String,
        approval_id: String,
        allow: bool,
    },
    TestSetup {
        endpoint: String,
        model: String,
        key: String,
    },
    SaveSetup {
        endpoint: String,
        model: String,
        key: String,
    },
    WifiScan,
    WifiConnect {
        path: String,
        name: String,
        passphrase: Option<String>,
    },
    Redraw,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Enter,
    Escape,
    Backspace,
    Tab,
    Up,
    Down,
    /// Alt+<letter> shortcuts (approval: a = allow, d = deny).
    Alt(char),
    Text(String),
}

/// Clickable areas, in surface coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Ask,
    AiStatus,
    Stop,
    Allow,
    Deny,
    Theme,
    SetupField(usize),
    SetupTest,
    SetupSave,
    SetupClose,
    Wifi,
    WifiRow(usize),
    WifiConnect,
    WifiScan,
    WifiClose,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub hit: Hit,
}

pub fn hit_test(areas: &[Area], x: f64, y: f64) -> Option<Hit> {
    let (x, y) = (x as f32, y as f32);
    areas
        .iter()
        .rev()
        .find(|a| x >= a.x && x < a.x + a.w && y >= a.y && y < a.y + a.h)
        .map(|a| a.hit)
}

pub struct Ui {
    pub panel: Panel,
    pub input: String,
    pub request: String,
    pub task_id: Option<String>,
    pub pending_start: bool,
    pub plan: Vec<PlanStep>,
    pub phase: Phase,
    pub narration: String,
    pub approvals: VecDeque<Approval>,
    pub link: Link,
    pub setup: SetupForm,
    pub wifi: WifiView,
    pub dark: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    pub fn new() -> Ui {
        Ui {
            panel: Panel::Hidden,
            input: String::new(),
            request: String::new(),
            task_id: None,
            pending_start: false,
            plan: Vec::new(),
            phase: Phase::Idle,
            narration: String::new(),
            approvals: VecDeque::new(),
            link: Link::Offline,
            setup: SetupForm::default(),
            wifi: WifiView::default(),
            dark: false,
        }
    }

    pub fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    pub fn working(&self) -> bool {
        matches!(self.phase, Phase::Starting | Phase::Working)
    }

    pub fn toggle_palette(&mut self) {
        self.panel = match self.panel {
            Panel::Hidden => Panel::Ask,
            _ => Panel::Hidden,
        };
    }

    pub fn open_wifi(&mut self) -> Vec<Command> {
        self.panel = Panel::Wifi;
        self.wifi.asking = false;
        self.wifi.password.clear();
        vec![Command::WifiScan, Command::Redraw]
    }

    /// New state from the Wi-Fi thread; keeps the selection on the same
    /// network when the list reorders.
    pub fn set_wifi(&mut self, state: WifiState) {
        let sel = self
            .wifi
            .state
            .networks
            .get(self.wifi.selected)
            .map(|n| n.path.clone());
        self.wifi.state = state;
        self.wifi.selected = sel
            .and_then(|p| self.wifi.state.networks.iter().position(|n| n.path == p))
            .unwrap_or(0);
    }

    pub fn open_setup(&mut self) {
        self.panel = Panel::Setup;
        self.setup.focus = 0;
    }

    /// The user submitted `text`; the app sends `task.start`.
    fn begin(&mut self, text: String) -> Command {
        self.request = text.clone();
        self.input.clear();
        self.task_id = None;
        self.pending_start = true;
        self.plan.clear();
        self.approvals.clear();
        self.phase = Phase::Starting;
        self.narration = "Working out a plan…".into();
        Command::Submit(text)
    }

    /// `task.start` answered.
    pub fn started(&mut self, result: Result<String, String>) {
        self.pending_start = false;
        match result {
            Ok(task_id) => self.task_id = Some(task_id),
            Err(e) => {
                self.phase = Phase::Failed;
                self.narration = e;
            }
        }
    }

    pub fn set_link(&mut self, link: Link) {
        self.link = link;
        if link == Link::Offline && self.working() {
            self.phase = Phase::Failed;
            self.narration = "Lost the connection to the AiOS service.".into();
            self.approvals.clear();
        }
    }

    pub fn apply(&mut self, msg: &EventMsg) {
        if self.task_id.as_deref() != Some(msg.task_id.as_str()) || !self.working() {
            return;
        }
        match &msg.event {
            Event::PlanCreated { steps } => {
                self.phase = Phase::Working;
                for s in steps {
                    self.plan.push(PlanStep {
                        id: s.id.clone(),
                        title: s.title.clone(),
                        state: StepState::Pending,
                        note: String::new(),
                    });
                }
                let n = self.plan.len();
                self.narration = format!("On it: {n} step{}.", if n == 1 { "" } else { "s" });
            }
            Event::StepStarted { step_id } => self.set_step(step_id, StepState::Running, None),
            Event::StepDone { step_id, summary } => {
                self.set_step(step_id, StepState::Done, Some(summary))
            }
            Event::StepFailed { step_id, reason } => {
                self.set_step(step_id, StepState::Failed, Some(reason))
            }
            Event::ApprovalNeeded {
                approval_id,
                action,
                args,
                risk,
                ..
            } => self.approvals.push_back(Approval {
                task_id: msg.task_id.clone(),
                approval_id: approval_id.clone(),
                action: action.clone(),
                args: args.to_string(),
                risk: *risk,
            }),
            Event::Result { summary } => {
                self.phase = Phase::Done;
                self.narration = summary.clone();
                self.approvals.clear();
            }
            Event::Error { message } => {
                self.phase = Phase::Failed;
                self.narration = message.clone();
                self.approvals.clear();
            }
            Event::Cancelled {} => {
                self.phase = Phase::Stopped;
                self.narration = "Stopped.".into();
                self.approvals.clear();
                for s in &mut self.plan {
                    if matches!(s.state, StepState::Pending | StepState::Running) {
                        s.state = StepState::Stopped;
                    }
                }
            }
            Event::Trace { .. } => {}
        }
    }

    fn set_step(&mut self, id: &str, state: StepState, note: Option<&String>) {
        if let Some(s) = self.plan.iter_mut().find(|s| s.id == id) {
            s.state = state;
            if let Some(n) = note {
                s.note = n.clone();
            }
        }
    }

    /// The step currently running, for the bottom status line.
    pub fn activity(&self) -> Option<String> {
        if !self.working() {
            return None;
        }
        if !self.approvals.is_empty() {
            return Some("Waiting for your approval…".into());
        }
        let running: Vec<&str> = self
            .plan
            .iter()
            .filter(|s| s.state == StepState::Running)
            .map(|s| s.title.as_str())
            .collect();
        Some(if running.is_empty() {
            self.narration.clone()
        } else {
            format!("{}…", running.join(", "))
        })
    }

    pub fn key(&mut self, key: Key) -> Vec<Command> {
        match self.panel {
            Panel::Hidden => vec![],
            Panel::Ask => self.ask_key(key),
            Panel::Setup => self.setup_key(key),
            Panel::Wifi => self.wifi_key(key),
        }
    }

    fn wifi_key(&mut self, key: Key) -> Vec<Command> {
        let n = self.wifi.state.networks.len();
        let w = &mut self.wifi;
        match key {
            Key::Escape if w.asking => {
                w.asking = false;
                w.password.clear();
                vec![Command::Redraw]
            }
            Key::Escape => {
                self.panel = Panel::Ask;
                vec![Command::Redraw]
            }
            Key::Up if !w.asking && n > 0 => {
                w.selected = (w.selected + n - 1) % n;
                vec![Command::Redraw]
            }
            Key::Down | Key::Tab if !w.asking && n > 0 => {
                w.selected = (w.selected + 1) % n;
                vec![Command::Redraw]
            }
            Key::Enter => self.click(Hit::WifiConnect),
            Key::Alt('r') => vec![Command::WifiScan, Command::Redraw],
            Key::Backspace if w.asking => {
                w.password.pop();
                vec![Command::Redraw]
            }
            Key::Text(t) if w.asking => {
                w.password.push_str(&t);
                vec![Command::Redraw]
            }
            _ => vec![],
        }
    }

    fn ask_key(&mut self, key: Key) -> Vec<Command> {
        match key {
            Key::Escape => {
                self.panel = Panel::Hidden;
                vec![Command::Redraw]
            }
            Key::Enter => {
                let text = self.input.trim().to_string();
                if text.is_empty() || self.working() {
                    return vec![Command::Redraw];
                }
                vec![self.begin(text)]
            }
            Key::Backspace => {
                self.input.pop();
                vec![Command::Redraw]
            }
            Key::Alt('a') | Key::Alt('d') => self.answer(key == Key::Alt('a')),
            Key::Alt('s') => {
                self.open_setup();
                vec![Command::Redraw]
            }
            Key::Alt('c') => self.click(Hit::Stop),
            Key::Alt('w') => self.open_wifi(),
            Key::Text(t) => {
                if !self.working() {
                    self.input.push_str(&t);
                }
                vec![Command::Redraw]
            }
            Key::Tab | Key::Alt(_) | Key::Up | Key::Down => vec![],
        }
    }

    fn setup_key(&mut self, key: Key) -> Vec<Command> {
        match key {
            Key::Escape => {
                self.panel = Panel::Ask;
                vec![Command::Redraw]
            }
            Key::Tab | Key::Down => {
                self.setup.focus = (self.setup.focus + 1) % 3;
                vec![Command::Redraw]
            }
            Key::Up => {
                self.setup.focus = (self.setup.focus + 2) % 3;
                vec![Command::Redraw]
            }
            Key::Enter => {
                if self.setup.focus < 2 {
                    self.setup.focus += 1;
                    vec![Command::Redraw]
                } else {
                    self.click(Hit::SetupSave)
                }
            }
            Key::Backspace => {
                self.setup.field_mut().pop();
                vec![Command::Redraw]
            }
            Key::Text(t) => {
                self.setup.field_mut().push_str(&t);
                vec![Command::Redraw]
            }
            Key::Alt(_) => vec![],
        }
    }

    fn answer(&mut self, allow: bool) -> Vec<Command> {
        match self.approvals.pop_front() {
            Some(a) => vec![
                Command::Approve {
                    task_id: a.task_id,
                    approval_id: a.approval_id,
                    allow,
                },
                Command::Redraw,
            ],
            None => vec![],
        }
    }

    pub fn click(&mut self, hit: Hit) -> Vec<Command> {
        match hit {
            Hit::Ask => {
                self.toggle_palette();
                vec![Command::Redraw]
            }
            Hit::AiStatus => {
                self.open_setup();
                vec![Command::Redraw]
            }
            Hit::Theme => {
                self.dark = !self.dark;
                vec![Command::Redraw]
            }
            Hit::Stop => match (&self.task_id, self.working()) {
                (Some(id), true) => vec![Command::Cancel(id.clone())],
                _ => vec![],
            },
            Hit::Allow => self.answer(true),
            Hit::Deny => self.answer(false),
            Hit::SetupField(i) => {
                self.setup.focus = i.min(2);
                vec![Command::Redraw]
            }
            Hit::SetupClose | Hit::WifiClose => {
                self.panel = Panel::Ask;
                vec![Command::Redraw]
            }
            Hit::Wifi => {
                if self.panel == Panel::Wifi {
                    self.panel = Panel::Hidden;
                    vec![Command::Redraw]
                } else {
                    self.open_wifi()
                }
            }
            Hit::WifiScan => vec![Command::WifiScan, Command::Redraw],
            Hit::WifiRow(i) => {
                if i < self.wifi.state.networks.len() && i != self.wifi.selected {
                    self.wifi.selected = i;
                    self.wifi.asking = false;
                    self.wifi.password.clear();
                }
                vec![Command::Redraw]
            }
            Hit::WifiConnect => {
                let w = &mut self.wifi;
                let Some(net) = w.state.networks.get(w.selected).cloned() else {
                    return vec![];
                };
                if net.connected {
                    return vec![];
                }
                if net.needs_password() && !w.asking {
                    w.asking = true;
                    return vec![Command::Redraw];
                }
                let passphrase = if w.asking {
                    if w.password.is_empty() {
                        return vec![];
                    }
                    Some(std::mem::take(&mut w.password))
                } else {
                    None
                };
                w.asking = false;
                w.state.message = format!("Connecting to {}…", net.name);
                vec![
                    Command::WifiConnect {
                        path: net.path,
                        name: net.name,
                        passphrase,
                    },
                    Command::Redraw,
                ]
            }
            Hit::SetupTest | Hit::SetupSave => {
                let f = &mut self.setup;
                if f.endpoint.trim().is_empty() || f.model.trim().is_empty() {
                    f.message = "Enter the endpoint (IP:port or URL) and the model name.".into();
                    return vec![Command::Redraw];
                }
                f.busy = true;
                f.message = "Testing the connection…".into();
                let (endpoint, model, key) = (
                    f.endpoint.trim().to_string(),
                    f.model.trim().to_string(),
                    f.key.clone(),
                );
                vec![
                    if hit == Hit::SetupSave {
                        Command::SaveSetup {
                            endpoint,
                            model,
                            key,
                        }
                    } else {
                        Command::TestSetup {
                            endpoint,
                            model,
                            key,
                        }
                    },
                    Command::Redraw,
                ]
            }
        }
    }

    pub fn setup_result(&mut self, message: String) {
        self.setup.busy = false;
        self.setup.message = message;
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn status_chip(ui: &Ui) -> (&'static str, fn(&Theme) -> Color) {
    match ui.link {
        Link::Offline => ("OFFLINE", |t| t.bad),
        _ if ui.working() => ("WORKING", |t| t.accent),
        Link::Online { ai: Some(false) } => ("AI NOT CONNECTED", |t| t.warn),
        Link::Online { .. } => ("READY", |t| t.ok),
    }
}

/// UTC wall clock: ("14:05", "TUE 21 OCT").
pub fn clock_strings(unix_secs: u64) -> (String, String) {
    let days = (unix_secs / 86_400) as i64;
    let secs = unix_secs % 86_400;
    let (hh, mm) = (secs / 3600, (secs % 3600) / 60);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let wd = (days + 4).rem_euclid(7) as usize; // 1970-01-01 was a Thursday
    const WD: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
    const MO: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    (
        format!("{hh:02}:{mm:02}"),
        format!("{} {} {}", WD[wd], d, MO[(m - 1) as usize]),
    )
}

/// The thin floating pill at the top of the screen.
pub fn draw_topbar(ui: &Ui, text: &mut Text, width: u32, unix_secs: u64) -> (Pixmap, Vec<Area>) {
    let th = ui.theme();
    let mut areas = Vec::new();
    let Some(mut pm) = Pixmap::new(width.max(1), TOPBAR_HEIGHT) else {
        return (unreachable_pixmap(), areas);
    };
    let label = TextStyle::mono(11.0);
    let (time, date) = clock_strings(unix_secs);
    let (chip_text, chip_color) = status_chip(ui);

    let mut items: Vec<(String, Option<Hit>)> = vec![
        ("AI · NATIVE OS".into(), None),
        ("ASK   SUPER+SPACE".into(), Some(Hit::Ask)),
        (chip_text.into(), Some(Hit::AiStatus)),
    ];
    if ui.wifi.state.adapter {
        let label = match ui.wifi.state.connected() {
            Some(n) => format!("WI-FI · {}", clip_chars(&n.name, 18)),
            None => "WI-FI".into(),
        };
        items.push((label, Some(Hit::Wifi)));
    }
    items.extend([
        (time, None),
        (date, None),
        (if ui.dark { "☾" } else { "☀" }.into(), Some(Hit::Theme)),
    ]);
    let gap = 18.0;
    let chip_pad = 10.0;
    let widths: Vec<f32> = items
        .iter()
        .map(|(s, _)| text.measure(s, label, None).0 + 2.0 * chip_pad)
        .collect();
    let stop_w = if ui.working() { 70.0 } else { 0.0 };
    let dot = 18.0;
    let pill_w =
        widths.iter().sum::<f32>() + gap * (items.len() as f32 - 1.0) + 32.0 + dot + stop_w;
    let pill_h = 36.0;
    let px = ((width as f32 - pill_w) / 2.0).max(4.0).round();
    let py = 10.0;
    gfx::panel(&mut pm, px, py, pill_w, pill_h, pill_h / 2.0, &th, 8.0);

    let mut x = px + 16.0;
    let cy = py + pill_h / 2.0;
    let dot_color = match ui.link {
        Link::Offline => th.bad,
        _ => th.ok,
    };
    gfx::circle(&mut pm, x + 4.0, cy, 4.0, dot_color);
    x += dot;
    for ((s, hit), w) in items.iter().zip(&widths) {
        let is_chip = hit.is_some();
        if is_chip {
            let col = if *hit == Some(Hit::AiStatus) {
                gfx::with_alpha(chip_color(&th), 0.16)
            } else {
                th.chip
            };
            gfx::rounded(&mut pm, x, py + 6.0, *w, pill_h - 12.0, 10.0, col);
        }
        let color = if *hit == Some(Hit::AiStatus) {
            chip_color(&th)
        } else if is_chip {
            th.text
        } else {
            th.text_dim
        };
        text.draw(&mut pm, s, x + chip_pad, cy - 8.0, label, color, None);
        if let Some(h) = hit {
            areas.push(Area {
                x,
                y: py,
                w: *w,
                h: pill_h,
                hit: *h,
            });
        }
        x += w + gap;
    }
    if ui.working() {
        let w = stop_w - 10.0;
        gfx::rounded(&mut pm, x - 6.0, py + 6.0, w, pill_h - 12.0, 10.0, th.bad);
        text.draw(
            &mut pm,
            "STOP",
            x + 8.0,
            cy - 8.0,
            label.bold(),
            Color::WHITE,
            None,
        );
        areas.push(Area {
            x: x - 6.0,
            y: py,
            w,
            h: pill_h,
            hit: Hit::Stop,
        });
    }
    (pm, areas)
}

fn clip_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).chain(['…']).collect()
    }
}

fn unreachable_pixmap() -> Pixmap {
    // Pixmap::new only fails for zero or absurd sizes; 1x1 cannot fail.
    match Pixmap::new(1, 1) {
        Some(p) => p,
        None => std::process::abort(),
    }
}

/// The palette card. Returns the pixmap (card plus shadow margin), its
/// clickable areas, and the caret position for the input field.
pub fn draw_palette(ui: &Ui, text: &mut Text) -> (Pixmap, Vec<Area>) {
    let th = ui.theme();
    let card_w = PALETTE_WIDTH as f32 - 2.0 * MARGIN;
    let inner_w = card_w - 2.0 * PAD;
    let mut content = Pixmap::new(PALETTE_WIDTH, 2000).unwrap_or_else(unreachable_pixmap);
    let mut areas = Vec::new();
    let x0 = MARGIN + PAD;
    let mut y = MARGIN + PAD;

    match ui.panel {
        Panel::Setup => y = draw_setup(ui, text, &mut content, &mut areas, x0, y, inner_w, &th),
        Panel::Wifi => y = draw_wifi(ui, text, &mut content, &mut areas, x0, y, inner_w, &th),
        _ => y = draw_ask(ui, text, &mut content, &mut areas, x0, y, inner_w, &th),
    }
    let card_h = (y - MARGIN + PAD - 8.0).ceil();
    let total_h = (card_h + 2.0 * MARGIN).ceil() as u32;
    let mut pm = Pixmap::new(PALETTE_WIDTH, total_h).unwrap_or_else(unreachable_pixmap);
    gfx::panel(&mut pm, MARGIN, MARGIN, card_w, card_h, 24.0, &th, 22.0);
    if let Some(sub) = content.clone_rect(
        tiny_skia::IntRect::from_xywh(0, 0, PALETTE_WIDTH, total_h).unwrap_or_else(|| {
            tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap_or_else(|| std::process::abort())
        }),
    ) {
        pm.draw_pixmap(
            0,
            0,
            sub.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
    }
    (pm, areas)
}

#[allow(clippy::too_many_arguments)]
fn draw_ask(
    ui: &Ui,
    text: &mut Text,
    pm: &mut Pixmap,
    areas: &mut Vec<Area>,
    x0: f32,
    mut y: f32,
    w: f32,
    th: &Theme,
) -> f32 {
    let big = TextStyle::sans(22.0);
    let label = TextStyle::mono(11.0);
    let editing = !ui.working() && (ui.request.is_empty() || !ui.input.is_empty());

    // Top line: the request (or the input field) with a status dot.
    let dot = match ui.phase {
        Phase::Failed => th.bad,
        Phase::Stopped => th.text_dim,
        Phase::Starting | Phase::Working => th.accent,
        _ => th.ok,
    };
    gfx::circle(pm, x0 + 5.0, y + 15.0, 5.0, dot);
    let tx = x0 + 22.0;
    if editing {
        let shown = if ui.input.is_empty() {
            "Ask anything…"
        } else {
            ui.input.as_str()
        };
        let color = if ui.input.is_empty() {
            th.text_dim
        } else {
            th.text
        };
        let h = text.draw(pm, shown, tx, y, big, color, Some(w - 22.0));
        // Caret after the text.
        let cw = if ui.input.is_empty() {
            0.0
        } else {
            text.measure(&ui.input, big, None).0
        };
        let caret_x = (tx + cw + 2.0).min(x0 + w);
        gfx::rounded(pm, caret_x, y + 3.0, 2.0, 24.0, 1.0, th.accent);
        y += h.max(30.0);
    } else {
        y += text
            .draw(pm, &ui.request, tx, y, big, th.text, Some(w - 22.0))
            .max(30.0);
    }

    if ui.request.is_empty() && ui.plan.is_empty() {
        y += 14.0;
        gfx::hline(pm, x0, y, w, gfx::with_alpha(th.text_dim, 0.25));
        y += 14.0;
        let hint = "Try: open browser · search rust news · play lofi for late night coding · note: buy milk · read todo.txt";
        y += text.draw(pm, hint, x0, y, TextStyle::sans(13.0), th.text_dim, Some(w));
        if ui.link == (Link::Online { ai: Some(false) }) {
            y += 8.0;
            y += text.draw(
                pm,
                "No AI connected: only these commands work. Press Alt+S to connect one.",
                x0,
                y,
                TextStyle::sans(13.0),
                th.warn,
                Some(w),
            );
        }
        if ui.link == Link::Offline {
            y += 8.0;
            y += text.draw(
                pm,
                "The AiOS service is not running yet. Retrying…",
                x0,
                y,
                TextStyle::sans(13.0),
                th.bad,
                Some(w),
            );
        }
        return y;
    }

    y += 14.0;
    gfx::hline(pm, x0, y, w, gfx::with_alpha(th.text_dim, 0.25));
    y += 16.0;
    text.draw(
        pm,
        "AI · THINKING OUT LOUD",
        x0,
        y,
        label,
        th.text_dim,
        None,
    );
    y += 20.0;
    y += text.draw(
        pm,
        &ui.narration,
        x0,
        y,
        TextStyle::serif_italic(16.0),
        th.text,
        Some(w),
    );

    if !ui.plan.is_empty() {
        y += 16.0;
        text.draw(pm, "PLAN", x0, y, label, th.text_dim, None);
        y += 22.0;
        for (i, step) in ui.plan.iter().enumerate() {
            let num = format!("{:02}", i + 1);
            text.draw(pm, &num, x0, y + 2.0, label, th.text_dim, None);
            let mark_x = x0 + 36.0;
            match step.state {
                StepState::Done => gfx::check(pm, mark_x, y + 10.0, th.ok),
                StepState::Running => {
                    gfx::circle(pm, mark_x, y + 10.0, 8.0, gfx::with_alpha(th.accent, 0.25));
                    gfx::circle(pm, mark_x, y + 10.0, 4.0, th.accent);
                }
                StepState::Failed => {
                    gfx::circle(pm, mark_x, y + 10.0, 8.0, th.bad);
                    text.draw(
                        pm,
                        "!",
                        mark_x - 2.5,
                        y + 2.0,
                        label.bold(),
                        Color::WHITE,
                        None,
                    );
                }
                StepState::Pending | StepState::Stopped => {
                    gfx::circle(pm, mark_x, y + 10.0, 8.0, gfx::with_alpha(th.text_dim, 0.3));
                    gfx::circle(pm, mark_x, y + 10.0, 6.5, th.panel);
                }
            }
            let title_color = match step.state {
                StepState::Pending | StepState::Stopped => th.text_dim,
                _ => th.text,
            };
            let tx = x0 + 56.0;
            y += text
                .draw(
                    pm,
                    &step.title,
                    tx,
                    y,
                    TextStyle::sans(15.0),
                    title_color,
                    Some(w - 56.0),
                )
                .max(22.0);
            if !step.note.is_empty() {
                let color = if step.state == StepState::Failed {
                    th.bad
                } else {
                    th.text_dim
                };
                y += text.draw(
                    pm,
                    &step.note,
                    tx,
                    y,
                    TextStyle::sans(12.0),
                    color,
                    Some(w - 56.0),
                );
            }
            y += 6.0;
        }
    }

    if let Some(a) = ui.approvals.front() {
        y += 10.0;
        let top = y;
        let body = format!("Allow {}?", a.action);
        let mut inner = y + 14.0;
        let mut tmp_y = inner;
        tmp_y += text
            .measure(&body, TextStyle::sans(16.0).bold(), Some(w - 28.0))
            .1;
        tmp_y += 6.0
            + text
                .measure(&a.args, TextStyle::mono(12.0), Some(w - 28.0))
                .1;
        let box_h = tmp_y - top + 58.0;
        gfx::rounded(pm, x0, top, w, box_h, 14.0, gfx::with_alpha(th.warn, 0.14));
        inner += text.draw(
            pm,
            &body,
            x0 + 14.0,
            inner,
            TextStyle::sans(16.0).bold(),
            th.text,
            Some(w - 28.0),
        );
        inner += 6.0;
        text.draw(
            pm,
            &a.args,
            x0 + 14.0,
            inner,
            TextStyle::mono(12.0),
            th.text_dim,
            Some(w - 28.0),
        );
        let by = top + box_h - 44.0;
        let bw = 96.0;
        let allow_x = x0 + w - 14.0 - bw;
        let deny_x = allow_x - 10.0 - bw;
        gfx::rounded(pm, deny_x, by, bw, 32.0, 10.0, th.chip);
        text.draw(
            pm,
            "Deny  Alt+D",
            deny_x + 12.0,
            by + 8.0,
            TextStyle::sans(13.0),
            th.text,
            None,
        );
        gfx::rounded(pm, allow_x, by, bw, 32.0, 10.0, th.accent);
        text.draw(
            pm,
            "Allow  Alt+A",
            allow_x + 10.0,
            by + 8.0,
            TextStyle::sans(13.0),
            Color::WHITE,
            None,
        );
        areas.push(Area {
            x: deny_x,
            y: by,
            w: bw,
            h: 32.0,
            hit: Hit::Deny,
        });
        areas.push(Area {
            x: allow_x,
            y: by,
            w: bw,
            h: 32.0,
            hit: Hit::Allow,
        });
        y = top + box_h;
    }

    y += 14.0;
    gfx::hline(pm, x0, y, w, gfx::with_alpha(th.text_dim, 0.25));
    y += 12.0;
    let footer = match ui.phase {
        Phase::Starting | Phase::Working => "Working",
        Phase::Done => "Done · type to ask something else",
        Phase::Failed => "Could not finish · type to try again",
        Phase::Stopped => "Stopped · type to ask something else",
        Phase::Idle => "",
    };
    text.draw(pm, footer, x0, y + 6.0, label, th.text_dim, None);
    if ui.working() {
        let bw = 84.0;
        let bx = x0 + w - bw;
        gfx::rounded(pm, bx, y, bw, 30.0, 10.0, th.bad);
        text.draw(
            pm,
            "Stop",
            bx + 26.0,
            y + 6.0,
            TextStyle::sans(14.0).bold(),
            Color::WHITE,
            None,
        );
        areas.push(Area {
            x: bx,
            y,
            w: bw,
            h: 30.0,
            hit: Hit::Stop,
        });
    }
    y + 30.0
}

#[allow(clippy::too_many_arguments)]
fn draw_wifi(
    ui: &Ui,
    text: &mut Text,
    pm: &mut Pixmap,
    areas: &mut Vec<Area>,
    x0: f32,
    mut y: f32,
    w: f32,
    th: &Theme,
) -> f32 {
    let v = &ui.wifi;
    let label = TextStyle::mono(11.0);
    y += text.draw(pm, "Wi-Fi", x0, y, TextStyle::sans(22.0), th.text, None);
    y += 6.0;
    let hint = if !v.state.adapter {
        "No Wi-Fi adapter found. Wired networking connects by itself."
    } else if v.asking {
        "Type the password and press Enter. Esc goes back."
    } else {
        "↑/↓ to choose, Enter to connect, Alt+R to scan again, Esc to close."
    };
    y += text.draw(pm, hint, x0, y, TextStyle::sans(13.0), th.text_dim, Some(w));
    y += 14.0;
    if v.state.adapter && v.state.networks.is_empty() {
        y += text.draw(
            pm,
            "No networks found yet…",
            x0,
            y,
            TextStyle::sans(15.0),
            th.text_dim,
            None,
        );
        y += 10.0;
    }
    for (i, n) in v.state.networks.iter().enumerate().take(8) {
        let selected = i == v.selected;
        if selected {
            gfx::rounded(pm, x0 - 8.0, y - 6.0, w + 16.0, 40.0, 10.0, th.field);
        }
        let bars: String = "▂▄▆█"
            .chars()
            .enumerate()
            .map(|(b, c)| if b < n.bars() { c } else { '·' })
            .collect();
        text.draw(
            pm,
            &bars,
            x0,
            y + 2.0,
            TextStyle::mono(13.0),
            th.text_dim,
            None,
        );
        text.draw(
            pm,
            &n.name,
            x0 + 52.0,
            y,
            TextStyle::sans(16.0),
            th.text,
            Some(w - 200.0),
        );
        let tag = if n.connected {
            "CONNECTED".to_string()
        } else if n.security == "open" {
            "OPEN".to_string()
        } else if n.known {
            "SAVED".to_string()
        } else {
            n.security.to_uppercase()
        };
        let tag_w = text.measure(&tag, label, None).0;
        let color = if n.connected { th.ok } else { th.text_dim };
        text.draw(pm, &tag, x0 + w - tag_w, y + 4.0, label, color, None);
        areas.push(Area {
            x: x0 - 8.0,
            y: y - 6.0,
            w: w + 16.0,
            h: 40.0,
            hit: Hit::WifiRow(i),
        });
        y += 40.0;
        if selected && v.asking {
            text.draw(pm, "PASSWORD", x0, y, label, th.text_dim, None);
            y += 18.0;
            gfx::rounded(pm, x0 - 2.0, y - 2.0, w + 4.0, 40.0, 11.0, th.accent);
            gfx::rounded(pm, x0, y, w, 36.0, 10.0, th.field);
            let masked: String = "•".repeat(v.password.chars().count());
            text.draw(
                pm,
                &masked,
                x0 + 12.0,
                y + 9.0,
                TextStyle::sans(15.0),
                th.text,
                None,
            );
            let cw = if masked.is_empty() {
                0.0
            } else {
                text.measure(&masked, TextStyle::sans(15.0), None).0
            };
            gfx::rounded(pm, x0 + 13.0 + cw, y + 8.0, 2.0, 20.0, 1.0, th.accent);
            y += 48.0;
        }
    }
    if !v.state.message.is_empty() {
        y += 6.0;
        y += text.draw(
            pm,
            &v.state.message,
            x0,
            y,
            TextStyle::sans(13.0),
            th.text_dim,
            Some(w),
        );
    }
    y += 14.0;
    let bw = 96.0;
    let connect_x = x0 + w - bw;
    let scan_x = connect_x - 10.0 - bw;
    for (bx, label_text, hit, fill, color) in [
        (x0, "Close", Hit::WifiClose, th.chip, th.text),
        (scan_x, "Scan", Hit::WifiScan, th.chip, th.text),
        (
            connect_x,
            "Connect",
            Hit::WifiConnect,
            th.accent,
            Color::WHITE,
        ),
    ] {
        if !v.state.adapter && hit != Hit::WifiClose {
            continue;
        }
        gfx::rounded(pm, bx, y, bw, 34.0, 10.0, fill);
        let tw = text.measure(label_text, TextStyle::sans(14.0), None).0;
        text.draw(
            pm,
            label_text,
            bx + (bw - tw) / 2.0,
            y + 9.0,
            TextStyle::sans(14.0),
            color,
            None,
        );
        areas.push(Area {
            x: bx,
            y,
            w: bw,
            h: 34.0,
            hit,
        });
    }
    y + 34.0
}

#[allow(clippy::too_many_arguments)]
fn draw_setup(
    ui: &Ui,
    text: &mut Text,
    pm: &mut Pixmap,
    areas: &mut Vec<Area>,
    x0: f32,
    mut y: f32,
    w: f32,
    th: &Theme,
) -> f32 {
    let label = TextStyle::mono(11.0);
    y += text.draw(
        pm,
        "Connect your AI",
        x0,
        y,
        TextStyle::sans(22.0),
        th.text,
        None,
    );
    y += 6.0;
    y += text.draw(
        pm,
        "Any OpenAI-compatible endpoint: a cloud API, a GPU box on your network (IP:port), or a local llama.cpp server.",
        x0,
        y,
        TextStyle::sans(13.0),
        th.text_dim,
        Some(w),
    );
    y += 14.0;
    let masked: String = "•".repeat(ui.setup.key.chars().count());
    let fields = [
        (
            "ENDPOINT",
            ui.setup.endpoint.as_str(),
            "http://192.168.1.20:8080/v1",
        ),
        ("MODEL", ui.setup.model.as_str(), "model name"),
        ("API KEY", masked.as_str(), "optional"),
    ];
    for (i, (name, value, placeholder)) in fields.iter().enumerate() {
        text.draw(pm, name, x0, y, label, th.text_dim, None);
        y += 18.0;
        let focused = ui.setup.focus == i;
        if focused {
            gfx::rounded(pm, x0 - 2.0, y - 2.0, w + 4.0, 40.0, 11.0, th.accent);
        }
        gfx::rounded(pm, x0, y, w, 36.0, 10.0, th.field);
        let (shown, color) = if value.is_empty() {
            (*placeholder, th.text_dim)
        } else {
            (*value, th.text)
        };
        text.draw(
            pm,
            shown,
            x0 + 12.0,
            y + 9.0,
            TextStyle::sans(15.0),
            color,
            Some(w - 24.0),
        );
        if focused {
            let cw = if value.is_empty() {
                0.0
            } else {
                text.measure(value, TextStyle::sans(15.0), None).0
            };
            gfx::rounded(pm, x0 + 13.0 + cw, y + 8.0, 2.0, 20.0, 1.0, th.accent);
        }
        areas.push(Area {
            x: x0,
            y,
            w,
            h: 36.0,
            hit: Hit::SetupField(i),
        });
        y += 48.0;
    }
    if !ui.setup.message.is_empty() {
        y += text.draw(
            pm,
            &ui.setup.message,
            x0,
            y,
            TextStyle::sans(13.0),
            th.text_dim,
            Some(w),
        );
        y += 10.0;
    }
    let bw = 96.0;
    let save_x = x0 + w - bw;
    let test_x = save_x - 10.0 - bw;
    let close_x = x0;
    gfx::rounded(pm, close_x, y, bw, 34.0, 10.0, th.chip);
    text.draw(
        pm,
        "Later",
        close_x + 28.0,
        y + 9.0,
        TextStyle::sans(14.0),
        th.text,
        None,
    );
    gfx::rounded(pm, test_x, y, bw, 34.0, 10.0, th.chip);
    text.draw(
        pm,
        "Test",
        test_x + 32.0,
        y + 9.0,
        TextStyle::sans(14.0),
        th.text,
        None,
    );
    gfx::rounded(pm, save_x, y, bw, 34.0, 10.0, th.accent);
    text.draw(
        pm,
        "Save",
        save_x + 30.0,
        y + 9.0,
        TextStyle::sans(14.0).bold(),
        Color::WHITE,
        None,
    );
    areas.push(Area {
        x: close_x,
        y,
        w: bw,
        h: 34.0,
        hit: Hit::SetupClose,
    });
    areas.push(Area {
        x: test_x,
        y,
        w: bw,
        h: 34.0,
        hit: Hit::SetupTest,
    });
    areas.push(Area {
        x: save_x,
        y,
        w: bw,
        h: 34.0,
        hit: Hit::SetupSave,
    });
    y + 34.0
}

/// The quiet status line at the bottom of the screen ("Opening …").
pub fn draw_status(ui: &Ui, text: &mut Text) -> Pixmap {
    let mut pm = Pixmap::new(STATUS_WIDTH, STATUS_HEIGHT).unwrap_or_else(unreachable_pixmap);
    let Some(line) = ui.activity() else {
        return pm; // fully transparent
    };
    let th = ui.theme();
    let style = TextStyle::sans(13.0);
    let (tw, _) = text.measure(&line, style, Some(STATUS_WIDTH as f32 - 80.0));
    let w = tw + 44.0;
    let x = ((STATUS_WIDTH as f32 - w) / 2.0).round();
    gfx::panel(&mut pm, x, 8.0, w, 30.0, 15.0, &th, 6.0);
    gfx::circle(&mut pm, x + 16.0, 23.0, 4.0, th.accent);
    text.draw(
        &mut pm,
        &line,
        x + 28.0,
        14.0,
        style,
        th.text_dim,
        Some(STATUS_WIDTH as f32 - 80.0),
    );
    pm
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::Step;
    use serde_json::json;

    fn ev(task: &str, seq: u64, event: Event) -> EventMsg {
        EventMsg {
            ts_ns: seq,
            task_id: task.into(),
            seq,
            event,
        }
    }

    fn typed(ui: &mut Ui, s: &str) {
        for c in s.chars() {
            ui.key(Key::Text(c.to_string()));
        }
    }

    #[test]
    fn submit_plan_approval_result() {
        let mut ui = Ui::new();
        ui.link = Link::Online { ai: Some(true) };
        assert!(ui.key(Key::Enter).is_empty()); // hidden: ignored
        ui.toggle_palette();
        typed(&mut ui, "open terminal");
        ui.key(Key::Backspace);
        typed(&mut ui, "l");
        let cmds = ui.key(Key::Enter);
        assert_eq!(cmds, vec![Command::Submit("open terminal".into())]);
        assert!(ui.working());
        ui.started(Ok("t1".into()));

        // Events for other tasks are ignored.
        ui.apply(&ev("other", 0, Event::Cancelled {}));
        assert!(ui.working());

        ui.apply(&ev(
            "t1",
            0,
            Event::PlanCreated {
                steps: vec![Step {
                    id: "s1".into(),
                    title: "Open the terminal".into(),
                    action: "app.launch".into(),
                    args: json!({"name": "terminal"}),
                }],
            },
        ));
        ui.apply(&ev(
            "t1",
            1,
            Event::StepStarted {
                step_id: "s1".into(),
            },
        ));
        assert_eq!(ui.activity().unwrap(), "Open the terminal…");
        ui.apply(&ev(
            "t1",
            2,
            Event::ApprovalNeeded {
                approval_id: "a1".into(),
                step_id: "s1".into(),
                action: "app.launch".into(),
                args: json!({"name": "terminal"}),
                risk: Risk::Ask,
            },
        ));
        assert_eq!(
            ui.key(Key::Alt('a'))[0],
            Command::Approve {
                task_id: "t1".into(),
                approval_id: "a1".into(),
                allow: true
            }
        );
        assert!(ui.approvals.is_empty());
        ui.apply(&ev(
            "t1",
            3,
            Event::StepDone {
                step_id: "s1".into(),
                summary: "Opened".into(),
            },
        ));
        ui.apply(&ev(
            "t1",
            4,
            Event::Result {
                summary: "Terminal is open.".into(),
            },
        ));
        assert_eq!(ui.phase, Phase::Done);
        assert_eq!(ui.plan[0].state, StepState::Done);
        assert_eq!(ui.narration, "Terminal is open.");
        assert!(ui.activity().is_none());
    }

    #[test]
    fn stop_and_cancelled() {
        let mut ui = Ui::new();
        ui.toggle_palette();
        typed(&mut ui, "play lofi");
        ui.key(Key::Enter);
        ui.started(Ok("t".into()));
        assert_eq!(ui.click(Hit::Stop), vec![Command::Cancel("t".into())]);
        assert_eq!(ui.key(Key::Alt('c')), vec![Command::Cancel("t".into())]);
        ui.apply(&ev("t", 0, Event::PlanCreated { steps: vec![] }));
        ui.apply(&ev("t", 1, Event::Cancelled {}));
        assert_eq!(ui.phase, Phase::Stopped);
        assert!(ui.click(Hit::Stop).is_empty());
    }

    #[test]
    fn offline_fails_running_task() {
        let mut ui = Ui::new();
        ui.toggle_palette();
        typed(&mut ui, "x");
        ui.key(Key::Enter);
        ui.set_link(Link::Offline);
        assert_eq!(ui.phase, Phase::Failed);
    }

    fn net(name: &str, security: &str, connected: bool) -> crate::wifi::Network {
        crate::wifi::Network {
            path: format!("/net/connman/iwd/0/3/{name}_{security}"),
            name: name.into(),
            security: security.into(),
            signal: -6000,
            connected,
            known: false,
        }
    }

    #[test]
    fn wifi_picker_asks_for_a_password_then_connects() {
        let mut ui = Ui::new();
        ui.toggle_palette();
        assert_eq!(ui.key(Key::Alt('w'))[0], Command::WifiScan);
        assert_eq!(ui.panel, Panel::Wifi);
        ui.set_wifi(WifiState {
            adapter: true,
            networks: vec![net("Home", "psk", false), net("Cafe", "open", false)],
            message: String::new(),
        });
        // Open network: connects without a password.
        ui.key(Key::Down);
        let cmds = ui.key(Key::Enter);
        assert!(
            matches!(&cmds[0], Command::WifiConnect { name, passphrase: None, .. } if name == "Cafe")
        );
        // Protected network: Enter asks, typing fills, Enter connects.
        ui.key(Key::Up);
        assert_eq!(ui.key(Key::Enter), vec![Command::Redraw]);
        assert!(ui.wifi.asking);
        for c in "hunter22".chars() {
            ui.key(Key::Text(c.to_string()));
        }
        let cmds = ui.key(Key::Enter);
        assert!(
            matches!(&cmds[0], Command::WifiConnect { name, passphrase: Some(p), .. }
            if name == "Home" && p == "hunter22")
        );
        assert!(ui.wifi.password.is_empty());
        // The list reorders: the selection follows the network.
        ui.set_wifi(WifiState {
            adapter: true,
            networks: vec![net("Cafe", "open", false), net("Home", "psk", true)],
            message: String::new(),
        });
        assert_eq!(ui.wifi.selected, 1);
        ui.key(Key::Escape);
        assert_eq!(ui.panel, Panel::Ask);
    }

    #[test]
    fn wifi_chip_only_with_an_adapter() {
        let mut text = Text::new();
        let mut ui = Ui::new();
        let (_, areas) = draw_topbar(&ui, &mut text, 1280, 0);
        assert!(!areas.iter().any(|a| a.hit == Hit::Wifi));
        ui.set_wifi(WifiState {
            adapter: true,
            networks: vec![net("Home", "psk", true)],
            message: String::new(),
        });
        let (_, areas) = draw_topbar(&ui, &mut text, 1280, 0);
        assert!(areas.iter().any(|a| a.hit == Hit::Wifi));
        ui.panel = Panel::Wifi;
        let (pm, areas) = draw_palette(&ui, &mut text);
        assert!(pm.height() > 100);
        assert!(areas.iter().any(|a| a.hit == Hit::WifiRow(0)));
    }

    #[test]
    fn alt_s_opens_setup_from_the_palette() {
        let mut ui = Ui::new();
        ui.toggle_palette();
        ui.key(Key::Alt('s'));
        assert_eq!(ui.panel, Panel::Setup);
        ui.key(Key::Escape);
        assert_eq!(ui.panel, Panel::Ask);
    }

    #[test]
    fn setup_form_flow() {
        let mut ui = Ui::new();
        ui.open_setup();
        typed(&mut ui, "10.0.2.2:8081");
        ui.key(Key::Tab);
        typed(&mut ui, "test-model");
        ui.key(Key::Enter); // next field
        typed(&mut ui, "sk-1");
        let cmds = ui.key(Key::Enter);
        assert_eq!(
            cmds[0],
            Command::SaveSetup {
                endpoint: "10.0.2.2:8081".into(),
                model: "test-model".into(),
                key: "sk-1".into()
            }
        );
        let mut empty = Ui::new();
        empty.open_setup();
        assert_eq!(empty.click(Hit::SetupSave), vec![Command::Redraw]);
    }

    #[test]
    fn clock() {
        // 2026-10-21 18:30 UTC was a Wednesday.
        let (t, d) = clock_strings(1_792_607_400);
        assert_eq!(t, "18:30");
        assert_eq!(d, "WED 21 OCT");
    }

    #[test]
    fn drawing_produces_hit_areas() {
        let mut text = Text::new();
        let mut ui = Ui::new();
        ui.link = Link::Online { ai: Some(false) };
        let (pm, areas) = draw_topbar(&ui, &mut text, 1280, 0);
        assert_eq!(pm.width(), 1280);
        assert!(areas.iter().any(|a| a.hit == Hit::Ask));
        let ask = areas.iter().find(|a| a.hit == Hit::Ask).unwrap();
        assert_eq!(
            hit_test(&areas, (ask.x + 2.0) as f64, (ask.y + 2.0) as f64),
            Some(Hit::Ask)
        );

        ui.toggle_palette();
        let (p1, _) = draw_palette(&ui, &mut text);
        typed(&mut ui, "do it");
        ui.key(Key::Enter);
        ui.started(Ok("t".into()));
        ui.apply(&ev(
            "t",
            0,
            Event::PlanCreated {
                steps: (1..=4)
                    .map(|i| Step {
                        id: format!("s{i}"),
                        title: format!("step {i}"),
                        action: "x".into(),
                        args: json!({}),
                    })
                    .collect(),
            },
        ));
        let (p2, areas) = draw_palette(&ui, &mut text);
        assert!(p2.height() > p1.height(), "plan makes the card taller");
        assert!(areas.iter().any(|a| a.hit == Hit::Stop));

        ui.open_setup();
        let (_, areas) = draw_palette(&ui, &mut text);
        assert!(areas.iter().any(|a| a.hit == Hit::SetupSave));
        let status = draw_status(&ui, &mut text);
        assert_eq!(status.width(), STATUS_WIDTH);
    }
}
