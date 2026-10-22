//! Wi-Fi through iwd's D-Bus API (net.connman.iwd), on its own thread.
//!
//! The shell's user is in group `netdev`, which iwd's D-Bus policy allows.
//! Passwords are handed to iwd by an agent object this thread serves; the
//! password lives only in memory until iwd asks for it once.

use crate::app::Msg;
use smithay_client_toolkit::reexports::calloop::channel;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zbus::blocking::{fdo::ObjectManagerProxy, Connection, Proxy};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const IWD: &str = "net.connman.iwd";
const AGENT_PATH: &str = "/org/aios/shell/wifi_agent";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Network {
    pub path: String,
    pub name: String,
    /// "open", "psk" or "8021x".
    pub security: String,
    /// dBm × 100, as iwd reports it.
    pub signal: i16,
    pub connected: bool,
    pub known: bool,
}

impl Network {
    pub fn needs_password(&self) -> bool {
        self.security == "psk" && !self.known && !self.connected
    }

    /// 0 to 4 bars.
    pub fn bars(&self) -> usize {
        match self.signal / 100 {
            s if s >= -55 => 4,
            s if s >= -67 => 3,
            s if s >= -75 => 2,
            s if s >= -85 => 1,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WifiState {
    /// iwd is reachable and has a station (Wi-Fi adapter in client mode).
    pub adapter: bool,
    pub networks: Vec<Network>,
    /// The last result or error, shown as is.
    pub message: String,
}

impl WifiState {
    pub fn connected(&self) -> Option<&Network> {
        self.networks.iter().find(|n| n.connected)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WifiCmd {
    Refresh,
    Scan,
    Connect {
        path: String,
        name: String,
        passphrase: Option<String>,
    },
}

struct Agent {
    passphrase: Arc<Mutex<Option<String>>>,
}

impl Agent {
    fn take(&self) -> zbus::fdo::Result<String> {
        self.passphrase
            .lock()
            .ok()
            .and_then(|mut p| p.take())
            .ok_or_else(|| zbus::fdo::Error::Failed("no password entered".into()))
    }
}

#[zbus::interface(name = "net.connman.iwd.Agent")]
impl Agent {
    fn release(&self) {}

    fn request_passphrase(&self, _network: ObjectPath<'_>) -> zbus::fdo::Result<String> {
        self.take()
    }

    fn request_private_key_passphrase(
        &self,
        _network: ObjectPath<'_>,
    ) -> zbus::fdo::Result<String> {
        self.take()
    }

    fn request_user_name_and_password(
        &self,
        _network: ObjectPath<'_>,
    ) -> zbus::fdo::Result<(String, String)> {
        Err(zbus::fdo::Error::NotSupported(
            "enterprise Wi-Fi is not supported yet".into(),
        ))
    }

    fn request_user_password(
        &self,
        _network: ObjectPath<'_>,
        _user: String,
    ) -> zbus::fdo::Result<String> {
        self.take()
    }

    fn cancel(&self, _reason: String) {
        if let Ok(mut p) = self.passphrase.lock() {
            *p = None;
        }
    }
}

type Props = HashMap<String, OwnedValue>;

fn prop_str(p: &Props, k: &str) -> String {
    match p.get(k).map(|v| &**v) {
        Some(Value::Str(s)) => s.as_str().to_string(),
        Some(Value::ObjectPath(o)) => o.as_str().to_string(),
        _ => String::new(),
    }
}

fn prop_bool(p: &Props, k: &str) -> bool {
    matches!(p.get(k).map(|v| &**v), Some(Value::Bool(true)))
}

struct Session {
    conn: Connection,
    passphrase: Arc<Mutex<Option<String>>>,
}

impl Session {
    fn open() -> zbus::Result<Session> {
        let conn = Connection::system()?;
        let passphrase = Arc::new(Mutex::new(None));
        conn.object_server().at(
            AGENT_PATH,
            Agent {
                passphrase: passphrase.clone(),
            },
        )?;
        let mgr = Proxy::new(
            &conn,
            IWD,
            "/net/connman/iwd",
            "net.connman.iwd.AgentManager",
        )?;
        mgr.call::<_, _, ()>("RegisterAgent", &(ObjectPath::try_from(AGENT_PATH)?,))?;
        Ok(Session { conn, passphrase })
    }

    fn station(&self) -> zbus::Result<Option<(OwnedObjectPath, HashMap<OwnedObjectPath, Props>)>> {
        let om = ObjectManagerProxy::builder(&self.conn)
            .destination(IWD)?
            .path("/")?
            .build()?;
        let objects = om.get_managed_objects()?;
        let mut station = None;
        let mut networks = HashMap::new();
        for (path, ifaces) in objects {
            for (iface, props) in ifaces {
                match iface.as_str() {
                    "net.connman.iwd.Station" if station.is_none() => station = Some(path.clone()),
                    "net.connman.iwd.Network" => {
                        networks.insert(path.clone(), props);
                    }
                    _ => {}
                }
            }
        }
        Ok(station.map(|s| (s, networks)))
    }

    fn state(&self) -> zbus::Result<WifiState> {
        let Some((station, props)) = self.station()? else {
            return Ok(WifiState::default());
        };
        let st = Proxy::new(&self.conn, IWD, station, "net.connman.iwd.Station")?;
        let ordered: Vec<(OwnedObjectPath, i16)> = st.call("GetOrderedNetworks", &())?;
        let networks = ordered
            .into_iter()
            .filter_map(|(path, signal)| {
                let p = props.get(&path)?;
                Some(Network {
                    path: path.as_str().to_string(),
                    name: prop_str(p, "Name"),
                    security: prop_str(p, "Type"),
                    signal,
                    connected: prop_bool(p, "Connected"),
                    known: !prop_str(p, "KnownNetwork").is_empty(),
                })
            })
            .collect();
        Ok(WifiState {
            adapter: true,
            networks,
            message: String::new(),
        })
    }

    fn scan(&self) -> zbus::Result<()> {
        if let Some((station, _)) = self.station()? {
            let st = Proxy::new(&self.conn, IWD, station, "net.connman.iwd.Station")?;
            st.call::<_, _, ()>("Scan", &())?;
        }
        Ok(())
    }

    fn connect(&self, path: &str, passphrase: Option<String>) -> zbus::Result<()> {
        if let Ok(mut p) = self.passphrase.lock() {
            *p = passphrase;
        }
        let net = Proxy::new(&self.conn, IWD, path, "net.connman.iwd.Network")?;
        let r = net.call::<_, _, ()>("Connect", &());
        if let Ok(mut p) = self.passphrase.lock() {
            *p = None;
        }
        r
    }
}

fn error_text(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, msg, _) => {
            let what = match name.as_str() {
                "net.connman.iwd.Failed" | "net.connman.iwd.Aborted" => "the connection failed",
                "net.connman.iwd.InvalidFormat" => "the password has the wrong format",
                "net.connman.iwd.NotSupported" => "this kind of network is not supported",
                "net.connman.iwd.Busy" => "Wi-Fi is busy, try again",
                _ => "",
            };
            match (what, msg) {
                ("", Some(m)) => m.clone(),
                ("", None) => name.as_str().to_string(),
                (w, _) => w.to_string(),
            }
        }
        other => other.to_string(),
    }
}

/// Start the Wi-Fi thread. It reports a fresh state every few seconds and
/// after every command.
pub fn spawn(tx: channel::Sender<Msg>) -> Sender<WifiCmd> {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || run(tx, cmd_rx));
    cmd_tx
}

fn run(tx: channel::Sender<Msg>, rx: Receiver<WifiCmd>) {
    let mut session: Option<Session> = None;
    let mut message = String::new();
    loop {
        let cmd = match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(c) => c,
            Err(RecvTimeoutError::Timeout) => WifiCmd::Refresh,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // No radio, no iwd (its unit only starts with one): stay quiet.
        let radio = std::fs::read_dir("/sys/class/ieee80211")
            .map(|mut d| d.next().is_some())
            .unwrap_or(false);
        if session.is_none() && radio {
            session = Session::open().ok();
        }
        let Some(s) = &session else {
            if tx.send(Msg::Wifi(WifiState::default())).is_err() {
                return;
            }
            continue;
        };
        match cmd {
            WifiCmd::Refresh => {}
            WifiCmd::Scan => {
                message = match s.scan() {
                    Ok(()) => "Scanning…".into(),
                    Err(e) => error_text(&e),
                }
            }
            WifiCmd::Connect {
                path,
                name,
                passphrase,
            } => {
                let _ = tx.send(Msg::Wifi(WifiState {
                    adapter: true,
                    networks: s.state().map(|st| st.networks).unwrap_or_default(),
                    message: format!("Connecting to {name}…"),
                }));
                message = match s.connect(&path, passphrase) {
                    Ok(()) => format!("Connected to {name}."),
                    Err(e) => format!("Could not connect to {name}: {}", error_text(&e)),
                };
            }
        }
        let state = match s.state() {
            Ok(mut st) => {
                st.message = message.clone();
                st
            }
            Err(_) => {
                // iwd went away (restarted, or no adapter yet): reconnect later.
                session = None;
                WifiState::default()
            }
        };
        if tx.send(Msg::Wifi(state)).is_err() {
            return;
        }
    }
}
