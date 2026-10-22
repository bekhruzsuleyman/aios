//! `notes.create`: one Markdown file per note, under the notes directory
//! only. The file name is derived from the title by keeping a safe subset
//! of characters, so a title can never name a path outside the directory.

use std::io::Write;
use std::path::{Path, PathBuf};

pub fn file_stem(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if (c == ' ' || c == '-' || c == '_') && !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.chars().count() >= 60 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "note".into()
    } else {
        out
    }
}

/// Create a new note; never overwrites an existing one.
pub fn create(dir: &Path, title: &str, body: &str) -> Result<PathBuf, String> {
    let stem = file_stem(title);
    for n in 0..1000 {
        let name = if n == 0 {
            format!("{stem}.md")
        } else {
            format!("{stem}-{n}.md")
        };
        let path = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                let text = if body.is_empty() {
                    format!("# {title}\n")
                } else {
                    format!("# {title}\n\n{body}\n")
                };
                f.write_all(text.as_bytes())
                    .map_err(|e| format!("writing note: {e}"))?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("creating note in {}: {e}", dir.display())),
        }
    }
    Err("too many notes with this title".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe() {
        assert_eq!(file_stem("Buy milk"), "buy-milk");
        assert_eq!(file_stem("../../etc/passwd"), "etcpasswd");
        assert_eq!(file_stem("/root/.ssh"), "rootssh");
        assert_eq!(file_stem("...."), "note");
        assert_eq!(file_stem("Ünïcode notes"), "ünïcode-notes");
    }

    #[test]
    fn creates_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("aios-notes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = create(&dir, "Buy milk", "2 litres").unwrap();
        let b = create(&dir, "Buy milk", "").unwrap();
        assert_ne!(a, b);
        assert_eq!(a.parent().unwrap(), dir);
        assert_eq!(
            std::fs::read_to_string(&a).unwrap(),
            "# Buy milk\n\n2 litres\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
