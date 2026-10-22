//! The permission broker: the only path from a requested action to its
//! execution (plan section 8).
//!
//! `decide` classifies a request with an in-memory policy lookup (no disk,
//! no network) and returns `Allowed` with a `Permit`, `NeedsApproval` with
//! a ticket, or `Denied`. A `Permit` can only be made here, and `execute`
//! is the only function that runs actions, so nothing reaches `actions`
//! without a decision. Approvals resolve only through `respond`, which
//! aiosd calls only for `approval.respond` on the shell's approval socket.
//! Every decision and every outcome is appended to the audit log.

use actions::{Action, Context, Output, ParseError};
use protocol::Risk;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq)]
pub struct ActionRequest {
    pub task_id: String,
    pub step_id: String,
    pub action: String,
    pub args: Value,
}

/// Proof that the broker allowed this exact request. Not constructible
/// outside this crate.
#[derive(Debug)]
pub struct Permit {
    req: ActionRequest,
    action: Action,
}

impl Permit {
    pub fn request(&self) -> &ActionRequest {
        &self.req
    }
}

pub struct Ticket {
    pub approval_id: String,
    pub risk: Risk,
    req: ActionRequest,
    action: Option<Action>,
    rx: oneshot::Receiver<bool>,
}

pub enum Decision {
    Allowed(Permit),
    NeedsApproval(Ticket),
    Denied(String),
}

/// Action name prefixes that are always denied: shell execution, raw
/// network access, writes outside the notes directory.
const NEVER_PREFIXES: &[&str] = &[
    "shell.",
    "exec",
    "process.",
    "system.",
    "fs.write",
    "fs.delete",
    "fs.move",
    "net.",
    "http.",
    "socket.",
];

/// Where the audit log goes.
pub enum AuditSink {
    File(PathBuf),
    Memory(Mutex<Vec<String>>),
}

struct Pending {
    task_id: String,
    tx: oneshot::Sender<bool>,
}

pub struct Broker {
    ctx: Context,
    audit: AuditSink,
    audit_lock: Mutex<()>,
    pending: Mutex<HashMap<String, Pending>>,
    unknown: Risk,
}

/// The policy table. Pure: no I/O. `unknown` is the risk of actions that
/// are not in the table (`[policy] default`, `ask` or `never`).
pub fn classify(
    name: &str,
    args: &Value,
    read_roots: &[PathBuf],
    unknown: Risk,
) -> Result<(Risk, Option<Action>), String> {
    if NEVER_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return Ok((Risk::Never, None));
    }
    match Action::parse(name, args) {
        Ok(action) => {
            let risk = match &action {
                Action::WebSearch { .. }
                | Action::MediaPlay { .. }
                | Action::NotesCreate { .. } => Risk::Free,
                // Section 8: every app launch is asked for.
                Action::AppLaunch { .. } => Risk::Ask,
                Action::FsRead { path } => {
                    if actions::fsread::lexically_inside(read_roots, path).is_some() {
                        Risk::Free
                    } else {
                        Risk::Never
                    }
                }
            };
            Ok((risk, Some(action)))
        }
        // Unlisted actions need approval (and have no implementation).
        Err(ParseError::Unknown(_)) => Ok((unknown, None)),
        Err(e @ ParseError::BadArgs(_)) => Err(e.to_string()),
    }
}

fn risk_str(r: Risk) -> &'static str {
    match r {
        Risk::Free => "free",
        Risk::Ask => "ask",
        Risk::Never => "never",
    }
}

impl Broker {
    pub fn new(ctx: Context, audit: AuditSink) -> Broker {
        Broker {
            ctx,
            audit,
            audit_lock: Mutex::new(()),
            pending: Mutex::new(HashMap::new()),
            unknown: Risk::Ask,
        }
    }

    /// Risk for actions not in the policy table (default `Ask`). `Free`
    /// is not accepted and falls back to `Ask`.
    pub fn with_unknown_risk(mut self, risk: Risk) -> Broker {
        self.unknown = if risk == Risk::Never {
            Risk::Never
        } else {
            Risk::Ask
        };
        self
    }

    pub fn context(&self) -> &Context {
        &self.ctx
    }

    fn log(&self, req: &ActionRequest, decision: &str, outcome: &str) {
        let line = json!({
            "ts": tracer::now_ns(),
            "unix_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            "task_id": req.task_id,
            "step_id": req.step_id,
            "action": req.action,
            "args": req.args,
            "decision": decision,
            "outcome": outcome,
        })
        .to_string();
        let _guard = self.audit_lock.lock();
        match &self.audit {
            AuditSink::File(path) => {
                let res = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .and_then(|mut f| writeln!(f, "{line}"));
                if let Err(e) = res {
                    eprintln!("broker: cannot write audit log {}: {e}", path.display());
                }
            }
            AuditSink::Memory(lines) => {
                if let Ok(mut l) = lines.lock() {
                    l.push(line);
                }
            }
        }
    }

    /// Audit lines (only for the in-memory sink; tests).
    pub fn audit_lines(&self) -> Vec<Value> {
        match &self.audit {
            AuditSink::Memory(lines) => lines
                .lock()
                .map(|l| {
                    l.iter()
                        .filter_map(|s| serde_json::from_str(s).ok())
                        .collect()
                })
                .unwrap_or_default(),
            AuditSink::File(path) => std::fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .filter_map(|s| serde_json::from_str(s).ok())
                .collect(),
        }
    }

    /// Decide on a request. The decision itself is an in-memory lookup;
    /// the audit line is written after it is made.
    pub fn decide(&self, req: ActionRequest) -> Decision {
        let decision = classify(&req.action, &req.args, &self.ctx.read_roots, self.unknown);
        match decision {
            Err(reason) => {
                self.log(&req, "denied", &format!("invalid: {reason}"));
                Decision::Denied(reason)
            }
            Ok((Risk::Never, _)) => {
                let reason = format!("{} is never allowed", req.action);
                self.log(&req, "denied", "policy: never");
                Decision::Denied(reason)
            }
            Ok((Risk::Free, Some(action))) => {
                self.log(&req, "allowed", "pending");
                Decision::Allowed(Permit { req, action })
            }
            Ok((risk, action)) => {
                let approval_id = uuid::Uuid::new_v4().to_string();
                let (tx, rx) = oneshot::channel();
                if let Ok(mut p) = self.pending.lock() {
                    p.insert(
                        approval_id.clone(),
                        Pending {
                            task_id: req.task_id.clone(),
                            tx,
                        },
                    );
                }
                self.log(
                    &req,
                    "needs_approval",
                    &format!("approval_id {approval_id} risk {}", risk_str(risk)),
                );
                Decision::NeedsApproval(Ticket {
                    approval_id,
                    risk,
                    req,
                    action,
                    rx,
                })
            }
        }
    }

    /// Wait for the user's answer to a ticket.
    pub async fn wait(&self, ticket: Ticket) -> Result<Permit, String> {
        let answer = ticket.rx.await;
        let req = ticket.req;
        match answer {
            Ok(true) => match ticket.action {
                Some(action) => {
                    self.log(&req, "approved", "pending");
                    Ok(Permit { req, action })
                }
                None => {
                    self.log(&req, "approved", "error: no such action");
                    Err(format!("{} is not an action AiOS has", req.action))
                }
            },
            Ok(false) => {
                self.log(&req, "denied_by_user", "not run");
                Err("you denied this action".into())
            }
            Err(_) => {
                self.log(&req, "revoked", "not run");
                Err("approval was withdrawn".into())
            }
        }
    }

    /// Answer a pending approval. Only aiosd's approval socket calls this.
    pub fn respond(&self, task_id: &str, approval_id: &str, allow: bool) -> bool {
        let Ok(mut p) = self.pending.lock() else {
            return false;
        };
        match p.get(approval_id) {
            Some(pending) if pending.task_id == task_id => {}
            _ => return false,
        }
        match p.remove(approval_id) {
            Some(pending) => pending.tx.send(allow).is_ok(),
            None => false,
        }
    }

    /// Revoke every pending approval of a task (on cancel).
    pub fn cancel_task(&self, task_id: &str) {
        if let Ok(mut p) = self.pending.lock() {
            p.retain(|_, v| v.task_id != task_id);
        }
    }

    pub fn pending_approvals(&self) -> usize {
        self.pending.lock().map(|p| p.len()).unwrap_or(0)
    }

    /// Run an allowed action. The only caller of `actions::execute`.
    pub async fn execute(&self, permit: Permit) -> Result<Output, String> {
        let result = actions::execute(&self.ctx, &permit.action).await;
        match &result {
            Ok(out) => self.log(&permit.req, "executed", &format!("ok: {}", out.summary)),
            Err(e) => self.log(&permit.req, "executed", &format!("error: {e}")),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actions::SearchProvider;

    fn broker(name: &str) -> (Broker, PathBuf) {
        let base = std::env::temp_dir().join(format!("aios-broker-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("user")).unwrap();
        std::fs::create_dir_all(base.join("notes")).unwrap();
        std::fs::write(base.join("user/todo.txt"), "milk").unwrap();
        std::fs::write(base.join("secret.txt"), "secret").unwrap();
        let ctx = Context {
            notes_dir: base.join("notes"),
            read_roots: vec![base.join("user")],
            launch_socket: base.join("launch.sock"),
            search: SearchProvider::BrowserFallback,
            http: reqwest::Client::new(),
        };
        (
            Broker::new(ctx, AuditSink::Memory(Mutex::new(Vec::new()))),
            base,
        )
    }

    fn req(action: &str, args: Value) -> ActionRequest {
        ActionRequest {
            task_id: "t".into(),
            step_id: "s1".into(),
            action: action.into(),
            args,
        }
    }

    #[test]
    fn policy_table() {
        let roots = vec![PathBuf::from("/var/lib/aios/user")];
        let risk = |a: &str, v: Value| classify(a, &v, &roots, Risk::Ask).map(|(r, _)| r);
        // free
        assert_eq!(risk("web.search", json!({"query": "rust"})), Ok(Risk::Free));
        assert_eq!(risk("media.play", json!({"query": "lofi"})), Ok(Risk::Free));
        assert_eq!(
            risk("notes.create", json!({"title": "a", "body": "b"})),
            Ok(Risk::Free)
        );
        assert_eq!(risk("fs.read", json!({"path": "todo.txt"})), Ok(Risk::Free));
        assert_eq!(
            risk("app.launch", json!({"name": "browser"})),
            Ok(Risk::Ask)
        );
        // ask
        assert_eq!(
            risk("app.launch", json!({"name": "terminal"})),
            Ok(Risk::Ask)
        );
        assert_eq!(risk("calendar.add", json!({"when": "now"})), Ok(Risk::Ask));
        // never
        assert_eq!(
            risk("fs.read", json!({"path": "/etc/shadow"})),
            Ok(Risk::Never)
        );
        assert_eq!(
            risk("fs.read", json!({"path": "../../etc/shadow"})),
            Ok(Risk::Never)
        );
        assert_eq!(
            risk("fs.read", json!({"path": "/var/lib/aios/user/../ai.key"})),
            Ok(Risk::Never)
        );
        assert_eq!(
            risk("fs.write", json!({"path": "/etc/x", "data": "y"})),
            Ok(Risk::Never)
        );
        assert_eq!(risk("shell.exec", json!({"cmd": "id"})), Ok(Risk::Never));
        assert_eq!(risk("exec", json!({"cmd": "id"})), Ok(Risk::Never));
        assert_eq!(
            risk("http.get", json!({"url": "http://x"})),
            Ok(Risk::Never)
        );
        assert_eq!(risk("net.connect", json!({"host": "x"})), Ok(Risk::Never));
        // malformed arguments are denied outright
        assert!(risk("app.launch", json!({"name": "sh -c id"})).is_err());
        assert!(risk("web.search", json!({"query": "x", "extra": 1})).is_err());
    }

    #[tokio::test]
    async fn free_action_runs_and_is_audited() {
        let (b, base) = broker("free");
        let Decision::Allowed(p) = b.decide(req(
            "notes.create",
            json!({"title": "Buy milk", "body": "2l"}),
        )) else {
            panic!("not allowed")
        };
        let out = b.execute(p).await.unwrap();
        assert_eq!(out.summary, "Saved note buy-milk.md");
        assert!(base.join("notes/buy-milk.md").exists());
        let lines = b.audit_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["decision"], "allowed");
        assert_eq!(lines[1]["decision"], "executed");
        for key in [
            "ts", "task_id", "step_id", "action", "args", "decision", "outcome",
        ] {
            assert!(lines[1].get(key).is_some(), "{key}");
        }
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn reads_inside_roots_untrusted_and_denials_are_audited() {
        let (b, base) = broker("read");
        let Decision::Allowed(p) = b.decide(req("fs.read", json!({"path": "todo.txt"}))) else {
            panic!()
        };
        let out = b.execute(p).await.unwrap();
        assert!(out.untrusted);
        assert_eq!(out.content.as_deref(), Some("milk"));

        let secret = base.join("secret.txt").to_string_lossy().into_owned();
        assert!(matches!(
            b.decide(req("fs.read", json!({"path": secret}))),
            Decision::Denied(_)
        ));
        // Symlink escape passes the lexical check but fails at execution.
        std::os::unix::fs::symlink(base.join("secret.txt"), base.join("user/link")).unwrap();
        let Decision::Allowed(p) = b.decide(req("fs.read", json!({"path": "link"}))) else {
            panic!()
        };
        assert!(b.execute(p).await.is_err());
        assert!(matches!(
            b.decide(req("shell.exec", json!({"cmd": "id"}))),
            Decision::Denied(_)
        ));
        let lines = b.audit_lines();
        let denied = lines.iter().filter(|l| l["decision"] == "denied").count();
        assert_eq!(denied, 2);
        assert!(lines.iter().any(|l| l["outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("error: path leads outside")));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn approvals_need_the_right_task_and_cancel_revokes() {
        let (b, base) = broker("ask");
        let Decision::NeedsApproval(t) = b.decide(req("app.launch", json!({"name": "terminal"})))
        else {
            panic!()
        };
        assert_eq!(t.risk, Risk::Ask);
        let id = t.approval_id.clone();
        assert!(
            !b.respond("other-task", &id, true),
            "wrong task cannot approve"
        );
        assert!(!b.respond("t", "made-up-id", true));
        assert!(b.respond("t", &id, false));
        assert!(b.wait(t).await.is_err());
        assert!(!b.respond("t", &id, true), "an answer is final");

        let Decision::NeedsApproval(t) = b.decide(req("app.launch", json!({"name": "terminal"})))
        else {
            panic!()
        };
        assert_eq!(b.pending_approvals(), 1);
        b.cancel_task("t");
        assert_eq!(b.pending_approvals(), 0);
        assert_eq!(b.wait(t).await.unwrap_err(), "approval was withdrawn");

        // Unknown actions can be approved but have nothing to run.
        let Decision::NeedsApproval(t) = b.decide(req("calendar.add", json!({}))) else {
            panic!()
        };
        let id = t.approval_id.clone();
        assert!(b.respond("t", &id, true));
        assert!(b.wait(t).await.is_err());
        let lines = b.audit_lines();
        assert!(lines.iter().any(|l| l["decision"] == "denied_by_user"));
        assert!(lines.iter().any(|l| l["decision"] == "revoked"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn decision_is_fast() {
        let roots = vec![PathBuf::from("/var/lib/aios/user")];
        let args = json!({"query": "rust"});
        let t0 = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = classify("web.search", &args, &roots, Risk::Ask);
        }
        let per = t0.elapsed() / 1000;
        assert_eq!(
            classify("calendar.add", &args, &roots, Risk::Never).map(|(r, _)| r),
            Ok(Risk::Never)
        );
        assert!(per.as_micros() < 1000, "decision took {per:?}");
    }
}
