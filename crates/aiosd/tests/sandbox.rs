//! `aiosd --check-sandbox` applies Landlock and seccomp to itself and then
//! tries a forbidden read and an exec. The exec must always fail; the read
//! must fail whenever the kernel enforces Landlock.

use std::process::Command;

#[test]
fn forbidden_exec_and_read_fail() {
    let base = std::env::temp_dir().join(format!("aiosd-sandbox-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("user")).unwrap();
    let cfg = base.join("aios.toml");
    std::fs::write(
        &cfg,
        format!(
            r#"[paths]
socket = "{b}/run/agent.sock"
approve = "{b}/run/approve.sock"
launch = "{b}/run/launch.sock"
audit = "{b}/audit.log"
notes = "{b}/notes"
read_roots = ["{b}/user"]
"#,
            b = base.display()
        ),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_aiosd"))
        .arg("--config")
        .arg(&cfg)
        .arg("--check-sandbox")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    eprintln!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
    let report: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(report["seccomp"], true);
    assert!(report["exec_usr_bin_true"]
        .as_str()
        .unwrap()
        .starts_with("denied"));
    assert!(report["read_notes_dir"].as_str().unwrap() == "ALLOWED");
    if report["landlock"] == "not_enforced" {
        eprintln!("this kernel does not enforce Landlock; read check skipped");
    } else {
        assert!(report["read_outside_allowed_dirs"]
            .as_str()
            .unwrap()
            .starts_with("denied"));
        assert!(out.status.success());
    }
}
