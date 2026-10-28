//! Shell state, input handling and drawing. Pure: no Wayland here, so it is
//! unit-tested on any host.

use crate::gfx::{self, Theme};
use crate::text::{Text, TextStyle};
use crate::wifi::WifiState;
use protocol::{Event, EventMsg, Risk};
use std::collections::VecDeque;
use tiny_skia::{Color, Pixmap};

/// The name in the top bar, set in the brand face (small caps style).
pub const BRAND: &str = "ALETHIC";
/// The top bar surface: the pill, plus room under it for the working pill.
pub const TOPBAR_HEIGHT: u32 = 84;
pub const PALETTE_WIDTH: u32 = 640;
pub const STATUS_WIDTH: u32 = 900;
pub const STATUS_HEIGHT: u32 = 48;
/// Transparent margin around the palette card for its shadow.
const MARGIN: f32 = 28.0;
const PAD: f32 = 22.0;

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
    /// The plan step asking (what the user reads); `action` and `args` are
    /// shown under it so the exact request is always visible.
    pub title: String,
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
    /// The top bar pill itself (outside its chips): opens the palette.
    Bar,
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
                step_id,
                action,
                args,
                risk,
            } => self.approvals.push_back(Approval {
                task_id: msg.task_id.clone(),
                title: self
                    .plan
                    .iter()
                    .find(|s| &s.id == step_id)
                    .map(|s| s.title.clone())
                    .unwrap_or_else(|| action.clone()),
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
            Hit::Ask | Hit::Bar => {
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
        Link::Offline => ("offline", |t| t.bad),
        _ if ui.working() => ("working", |t| t.accent),
        Link::Online { ai: Some(false) } => ("ai not connected", |t| t.warn),
        Link::Online { .. } => ("ready", |t| t.ok),
    }
}

/// UTC wall clock: ("14:05", "tue 21 oct").
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
    const WD: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
    const MO: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    (
        format!("{hh:02}:{mm:02}"),
        format!("{} {} {}", WD[wd], d, MO[(m - 1) as usize]),
    )
}

/// The thin floating pill at the top of the screen, and below it, while a
/// task runs, the small "working · stop" pill.
pub fn draw_topbar(ui: &Ui, text: &mut Text, width: u32, unix_secs: u64) -> (Pixmap, Vec<Area>) {
    let th = ui.theme();
    let mut areas = Vec::new();
    let Some(mut pm) = Pixmap::new(width.max(1), TOPBAR_HEIGHT) else {
        return (unreachable_pixmap(), areas);
    };
    let brand = TextStyle::mono(10.0);
    let label = TextStyle::sans(12.0);
    let keycap = TextStyle::mono(9.5);
    let (time, date) = clock_strings(unix_secs);
    let (chip_text, chip_color) = status_chip(ui);

    // What goes in the bar, left to right.
    enum Item {
        Brand,
        Ask,
        Chip(String, Option<Color>, Hit),
        Clock(String),
    }
    let mut items = vec![
        Item::Brand,
        Item::Ask,
        Item::Chip(chip_text.into(), Some(chip_color(&th)), Hit::AiStatus),
    ];
    if ui.wifi.state.adapter {
        let label = match ui.wifi.state.connected() {
            Some(n) => format!("wi-fi · {}", clip_chars(&n.name, 18)),
            None => "wi-fi".into(),
        };
        items.push(Item::Chip(label, None, Hit::Wifi));
    }
    items.push(Item::Clock(format!("{time}  ·  {date}")));
    items.push(Item::Chip(
        if ui.dark { "◑ dark" } else { "◑ light" }.into(),
        None,
        Hit::Theme,
    ));

    let pad = 10.0;
    let gap = 9.0; // on each side of the hairline between items
    let ask_key = "super space";
    let widths: Vec<f32> = items
        .iter()
        .map(|it| match it {
            Item::Brand => 14.0 + text.measure(BRAND, brand, None).0,
            Item::Ask => {
                text.measure("Ask", label, None).0
                    + 8.0
                    + text.measure(ask_key, keycap, None).0
                    + 10.0
                    + 2.0 * pad
            }
            Item::Chip(s, dot, _) => {
                text.measure(s, label, None).0 + 2.0 * pad + if dot.is_some() { 12.0 } else { 0.0 }
            }
            Item::Clock(s) => text.measure(s, label, None).0,
        })
        .collect();
    let pill_h = 32.0;
    let pill_w = widths.iter().sum::<f32>() + 2.0 * gap * (items.len() as f32 - 1.0) + 2.0 * 14.0;
    let px = ((width as f32 - pill_w) / 2.0).max(4.0).round();
    let py = 8.0;
    gfx::panel(&mut pm, px, py, pill_w, pill_h, pill_h / 2.0, &th, 8.0);
    areas.push(Area {
        x: px,
        y: py,
        w: pill_w,
        h: pill_h,
        hit: Hit::Bar,
    });

    let cy = py + pill_h / 2.0;
    let hair = gfx::with_alpha(th.text_dim, 0.18);
    let mut x = px + 14.0;
    for (i, (it, w)) in items.iter().zip(&widths).enumerate() {
        if i > 0 {
            gfx::vline(&mut pm, x - gap - 0.5, cy - 7.0, 14.0, hair);
        }
        let chip_y = py + 5.0;
        let chip_h = pill_h - 10.0;
        match it {
            Item::Brand => {
                let dot = match ui.link {
                    Link::Offline => th.bad,
                    _ => th.ok,
                };
                gfx::circle(&mut pm, x + 3.0, cy, 3.0, dot);
                text.draw(
                    &mut pm,
                    BRAND,
                    x + 14.0,
                    cy - 7.0,
                    brand,
                    th.text_dim,
                    None,
                );
            }
            Item::Ask => {
                gfx::outline(&mut pm, x, chip_y, *w, chip_h, chip_h / 2.0, hair, th.panel);
                text.draw(
                    &mut pm,
                    "Ask",
                    x + pad,
                    cy - 8.5,
                    label.medium(),
                    th.text,
                    None,
                );
                let kx = x + pad + text.measure("Ask", label.medium(), None).0 + 8.0;
                let kw = text.measure(ask_key, keycap, None).0 + 10.0;
                gfx::rounded(&mut pm, kx, cy - 8.0, kw, 16.0, 5.0, th.chip);
                text.draw(
                    &mut pm,
                    ask_key,
                    kx + 5.0,
                    cy - 7.0,
                    keycap,
                    th.text_dim,
                    None,
                );
                areas.push(Area {
                    x,
                    y: py,
                    w: *w,
                    h: pill_h,
                    hit: Hit::Ask,
                });
            }
            Item::Chip(s, dot, hit) => {
                gfx::outline(&mut pm, x, chip_y, *w, chip_h, chip_h / 2.0, hair, th.panel);
                let mut tx = x + pad;
                if let Some(c) = dot {
                    gfx::circle(&mut pm, tx + 3.0, cy, 3.0, *c);
                    tx += 12.0;
                }
                text.draw(&mut pm, s, tx, cy - 8.5, label, th.text, None);
                areas.push(Area {
                    x,
                    y: py,
                    w: *w,
                    h: pill_h,
                    hit: *hit,
                });
            }
            Item::Clock(s) => {
                text.draw(&mut pm, s, x, cy - 8.5, label, th.text_dim, None);
            }
        }
        x += w + 2.0 * gap;
    }

    // The working pill, hanging under the bar while a task runs.
    if ui.working() {
        let caption = if ui.approvals.is_empty() {
            "AI WORKING"
        } else {
            "AI WAITING FOR YOU"
        };
        let cap = TextStyle::mono(9.5);
        let stop = TextStyle::sans(11.0);
        let cw = text.measure(caption, cap, None).0;
        let sw = text.measure("stop", stop, None).0 + 16.0;
        let h = 24.0;
        let w = 12.0 + 6.0 + 8.0 + cw + 10.0 + sw + 4.0;
        let x = ((width as f32 - w) / 2.0).round();
        let y = py + pill_h + 8.0;
        gfx::panel(&mut pm, x, y, w, h, h / 2.0, &th, 6.0);
        gfx::circle(&mut pm, x + 15.0, y + h / 2.0, 3.0, th.accent);
        text.draw(&mut pm, caption, x + 26.0, y + 6.0, cap, th.text_dim, None);
        let bx = x + w - sw - 4.0;
        gfx::rounded(&mut pm, bx, y + 4.0, sw, h - 8.0, (h - 8.0) / 2.0, th.chip);
        text.draw(&mut pm, "stop", bx + 8.0, y + 4.5, stop, th.text, None);
        areas.push(Area {
            x,
            y,
            w,
            h,
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
    let big = TextStyle::sans(17.0);
    let label = TextStyle::mono(9.5);
    let row = TextStyle::sans(13.0);
    let hair = gfx::with_alpha(th.text_dim, 0.16);
    let editing = !ui.working() && (ui.request.is_empty() || !ui.input.is_empty());

    // Top line: the request (or the input field) after a small status dot.
    let dot = match ui.phase {
        Phase::Failed => th.bad,
        Phase::Stopped => th.text_dim,
        _ => th.ok,
    };
    gfx::circle(pm, x0 + 4.0, y + 11.5, 3.0, dot);
    let tx = x0 + 18.0;
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
        let h = text.draw(pm, shown, tx, y, big, color, Some(w - 18.0));
        let cw = if ui.input.is_empty() {
            0.0
        } else {
            text.measure(&ui.input, big, None).0
        };
        let caret_x = (tx + cw + 1.5).min(x0 + w);
        gfx::rounded(pm, caret_x, y + 2.0, 1.5, 20.0, 0.75, th.text);
        y += h.max(23.0);
    } else {
        y += text
            .draw(pm, &ui.request, tx, y, big, th.text, Some(w - 18.0))
            .max(23.0);
    }

    if ui.request.is_empty() && ui.plan.is_empty() {
        y += 14.0;
        gfx::hline(pm, x0, y, w, hair);
        y += 14.0;
        let hints = [
            "open browser",
            "search rust news",
            "play lofi for late night coding",
            "note: buy milk",
            "read todo.txt",
        ];
        text.draw(pm, "TRY", x0, y + 1.0, label, th.text_dim, None);
        y += 20.0;
        for h in hints {
            y += text.draw(pm, h, x0, y, row, th.text_dim, Some(w)).max(20.0) + 2.0;
        }
        let note = match ui.link {
            Link::Online { ai: Some(false) } => Some((
                "No AI connected: only these commands work. Alt+S connects one.",
                th.warn,
            )),
            Link::Offline => Some(("The AiOS service is not running yet. Retrying…", th.bad)),
            _ => None,
        };
        if let Some((n, c)) = note {
            y += 8.0;
            y += text.draw(pm, n, x0, y, TextStyle::sans(12.0), c, Some(w));
        }
        return y;
    }

    y += 14.0;
    gfx::hline(pm, x0, y, w, hair);
    y += 16.0;
    gfx::circle(pm, x0 + 3.0, y + 6.0, 2.5, th.ok);
    text.draw(
        pm,
        "AI · THINKING OUT LOUD",
        x0 + 12.0,
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
        TextStyle::serif_italic(19.5),
        th.text,
        Some(w),
    );

    if !ui.plan.is_empty() {
        y += 18.0;
        text.draw(pm, "PLAN", x0, y, label, th.text_dim, None);
        y += 20.0;
        let mark_x = x0 + w - 10.0;
        for (i, step) in ui.plan.iter().enumerate() {
            let num = format!("{:02}", i + 1);
            text.draw(pm, &num, x0, y + 2.0, label, th.text_dim, None);
            let title_color = match step.state {
                StepState::Pending | StepState::Stopped => th.text_dim,
                _ => th.text,
            };
            let tx = x0 + 28.0;
            let tw = w - 28.0 - 70.0;
            let cy = y + 9.0;
            match step.state {
                StepState::Done => gfx::tick(pm, mark_x, cy, th.ok),
                StepState::Running => gfx::circle(pm, mark_x, cy, 3.0, th.accent),
                StepState::Failed => {
                    let fw = text.measure("failed", label, None).0;
                    text.draw(
                        pm,
                        "failed",
                        mark_x + 5.0 - fw,
                        cy - 6.0,
                        label,
                        th.bad,
                        None,
                    );
                }
                StepState::Stopped => {
                    let fw = text.measure("stopped", label, None).0;
                    text.draw(
                        pm,
                        "stopped",
                        mark_x + 5.0 - fw,
                        cy - 6.0,
                        label,
                        th.text_dim,
                        None,
                    );
                }
                StepState::Pending => {}
            }
            y += text
                .draw(pm, &step.title, tx, y, row, title_color, Some(tw))
                .max(18.0);
            if step.state == StepState::Failed && !step.note.is_empty() {
                y += text.draw(
                    pm,
                    &step.note,
                    tx,
                    y,
                    TextStyle::sans(11.5),
                    th.bad,
                    Some(tw),
                );
            }
            y += 9.0;
        }
        y -= 9.0;
    }

    if let Some(a) = ui.approvals.front() {
        y += 18.0;
        let top = y;
        let body = format!("{}?", a.title);
        let detail = format!("{} · {}", a.action, a.args);
        let tw = w - 28.0;
        let bh = text
            .measure(&body, TextStyle::sans(13.5).medium(), Some(tw))
            .1;
        let ah = text.measure(&detail, TextStyle::mono(10.5), Some(tw)).1;
        let box_h = 14.0 + bh + 4.0 + ah + 12.0 + 28.0 + 14.0;
        gfx::outline(pm, x0, top, w, box_h, 14.0, hair, th.field);
        let mut inner = top + 14.0;
        inner += text.draw(
            pm,
            &body,
            x0 + 14.0,
            inner,
            TextStyle::sans(13.5).medium(),
            th.text,
            Some(tw),
        );
        inner += 4.0;
        text.draw(
            pm,
            &detail,
            x0 + 14.0,
            inner,
            TextStyle::mono(10.5),
            th.text_dim,
            Some(tw),
        );
        let by = top + box_h - 14.0 - 28.0;
        let small = TextStyle::sans(12.0);
        let allow_w = text.measure("Allow   alt a", small, None).0 + 24.0;
        let deny_w = text.measure("Deny   alt d", small, None).0 + 24.0;
        let allow_x = x0 + w - 14.0 - allow_w;
        let deny_x = allow_x - 8.0 - deny_w;
        gfx::outline(pm, deny_x, by, deny_w, 28.0, 14.0, hair, th.panel);
        text.draw(pm, "Deny", deny_x + 12.0, by + 6.0, small, th.text, None);
        let dk = text.measure("Deny", small, None).0;
        text.draw(
            pm,
            "alt d",
            deny_x + 12.0 + dk + 10.0,
            by + 7.5,
            TextStyle::mono(9.5),
            th.text_dim,
            None,
        );
        gfx::rounded(pm, allow_x, by, allow_w, 28.0, 14.0, th.text);
        text.draw(
            pm,
            "Allow",
            allow_x + 12.0,
            by + 6.0,
            small.medium(),
            th.panel_solid,
            None,
        );
        let ak = text.measure("Allow", small.medium(), None).0;
        text.draw(
            pm,
            "alt a",
            allow_x + 12.0 + ak + 10.0,
            by + 7.5,
            TextStyle::mono(9.5),
            gfx::with_alpha(th.panel_solid, 0.6),
            None,
        );
        areas.push(Area {
            x: deny_x,
            y: by,
            w: deny_w,
            h: 28.0,
            hit: Hit::Deny,
        });
        areas.push(Area {
            x: allow_x,
            y: by,
            w: allow_w,
            h: 28.0,
            hit: Hit::Allow,
        });
        y = top + box_h;
    }

    // A short closing line only when something needs saying.
    let footer = match ui.phase {
        Phase::Failed => Some("could not finish · type to try again"),
        Phase::Stopped => Some("stopped · type to ask something else"),
        _ => None,
    };
    if let Some(f) = footer {
        y += 16.0;
        text.draw(pm, f, x0, y, label, th.text_dim, None);
        y += 14.0;
    }
    y + 4.0
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
    // Quoted parts (“…”) are the step's subject: drawn stronger, like the
    // bold keywords in the design ("Opening YouTube · searching lofi…").
    let plain = TextStyle::sans(12.5);
    let strong = TextStyle::sans(12.5).medium();
    let mut parts: Vec<(String, bool)> = Vec::new();
    for (i, seg) in line.split(['“', '”']).enumerate() {
        if !seg.is_empty() {
            parts.push((seg.to_string(), i % 2 == 1));
        }
    }
    let max_w = STATUS_WIDTH as f32 - 80.0;
    let widths: Vec<f32> = parts
        .iter()
        .map(|(t, b)| text.measure(t, if *b { strong } else { plain }, None).0)
        .collect();
    let tw = widths.iter().sum::<f32>().min(max_w);
    let w = tw + 40.0;
    let x = ((STATUS_WIDTH as f32 - w) / 2.0).round();
    gfx::panel(&mut pm, x, 8.0, w, 28.0, 14.0, &th, 6.0);
    gfx::circle(&mut pm, x + 15.0, 22.0, 3.0, th.accent);
    let mut tx = x + 26.0;
    for ((t, b), pw) in parts.iter().zip(&widths) {
        if tx - x - 26.0 >= max_w {
            break;
        }
        let (style, color) = if *b {
            (strong, th.text)
        } else {
            (plain, th.text_dim)
        };
        text.draw(&mut pm, t, tx, 14.5, style, color, None);
        tx += pw;
    }
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
        assert_eq!(d, "wed 21 oct");
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
        let (p2, _) = draw_palette(&ui, &mut text);
        assert!(p2.height() > p1.height(), "plan makes the card taller");
        let (_, bar) = draw_topbar(&ui, &mut text, 1280, 0);
        assert!(
            bar.iter().any(|a| a.hit == Hit::Stop),
            "working pill has stop"
        );

        ui.open_setup();
        let (_, areas) = draw_palette(&ui, &mut text);
        assert!(areas.iter().any(|a| a.hit == Hit::SetupSave));
        let status = draw_status(&ui, &mut text);
        assert_eq!(status.width(), STATUS_WIDTH);
    }
}

/// Design previews: renders the shell's surfaces over the backdrop into
/// `target/preview/*.png` so the look can be checked without a VM.
/// `cargo test -p aios-shell --release preview -- --ignored`
#[cfg(test)]
mod preview {
    use super::*;
    use protocol::Step;
    use serde_json::json;

    fn compose(ui: &Ui, text: &mut Text, name: &str) {
        let (w, h) = (1280u32, 800u32);
        let Some(mut screen) = gfx::backdrop(w, h, ui.dark) else {
            return;
        };
        let paint = tiny_skia::PixmapPaint::default();
        let id = tiny_skia::Transform::identity();
        let (bar, _) = draw_topbar(ui, text, w, 1_792_607_400);
        screen.draw_pixmap(0, 0, bar.as_ref(), &paint, id, None);
        if ui.panel != Panel::Hidden {
            let (p, _) = draw_palette(ui, text);
            let x = (w as i32 - p.width() as i32) / 2;
            let y = (h as i32 - p.height() as i32) / 2;
            screen.draw_pixmap(x, y, p.as_ref(), &paint, id, None);
        }
        let st = draw_status(ui, text);
        screen.draw_pixmap(
            (w as i32 - st.width() as i32) / 2,
            h as i32 - st.height() as i32 - 12,
            st.as_ref(),
            &paint,
            id,
            None,
        );
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/preview");
        let _ = std::fs::create_dir_all(&dir);
        let _ = screen.save_png(dir.join(format!("{name}.png")));
    }

    fn step(i: usize, title: &str) -> Step {
        Step {
            id: format!("s{i}"),
            title: title.into(),
            action: "x".into(),
            args: json!({}),
        }
    }

    fn ev(seq: u64, event: Event) -> EventMsg {
        EventMsg {
            ts_ns: seq,
            task_id: "t".into(),
            seq,
            event,
        }
    }

    #[test]
    #[ignore]
    fn preview() {
        let mut text = Text::new();
        let mut ui = Ui::new();
        ui.link = Link::Online { ai: Some(true) };
        compose(&ui, &mut text, "1-desktop");
        ui.toggle_palette();
        compose(&ui, &mut text, "2-palette-empty");
        for c in "open youtube and play lofi for late night coding".chars() {
            ui.key(Key::Text(c.to_string()));
        }
        compose(&ui, &mut text, "3-palette-typed");
        ui.key(Key::Enter);
        ui.started(Ok("t".into()));
        ui.apply(&ev(
            0,
            Event::PlanCreated {
                steps: vec![
                    step(1, "Open the browser"),
                    step(2, "Search YouTube for “lofi for late night coding”"),
                    step(3, "Save a note “Late night playlist”"),
                    step(4, "Show the results"),
                ],
            },
        ));
        ui.apply(&ev(
            1,
            Event::StepStarted {
                step_id: "s1".into(),
            },
        ));
        ui.apply(&ev(
            2,
            Event::StepDone {
                step_id: "s1".into(),
                summary: "Opened the browser".into(),
            },
        ));
        ui.apply(&ev(
            3,
            Event::StepStarted {
                step_id: "s2".into(),
            },
        ));
        compose(&ui, &mut text, "4-plan-running");
        ui.apply(&ev(
            4,
            Event::ApprovalNeeded {
                approval_id: "a".into(),
                step_id: "s2".into(),
                action: "app.launch".into(),
                args: json!({"name": "browser"}),
                risk: Risk::Ask,
            },
        ));
        compose(&ui, &mut text, "5-approval");
    }
}
