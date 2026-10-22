//! Intent router: simple commands go straight to one action, no AI.
//! Patterns are case-insensitive and anchored at the start.

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Routed {
    pub title: String,
    pub action: &'static str,
    pub args: Value,
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| text[prefix.len()..].trim())
        .filter(|rest| !rest.is_empty())
}

pub fn route(text: &str) -> Option<Routed> {
    let t = text.trim();
    if let Some(rest) = strip_prefix_ci(t, "note:") {
        let title: String = rest
            .lines()
            .next()
            .unwrap_or(rest)
            .split_whitespace()
            .take(8)
            .collect::<Vec<_>>()
            .join(" ");
        return Some(Routed {
            title: format!("Save a note “{title}”"),
            action: "notes.create",
            args: json!({"title": title, "body": rest}),
        });
    }
    if let Some(rest) = strip_prefix_ci(t, "open ") {
        return Some(Routed {
            title: format!("Open {rest}"),
            action: "app.launch",
            args: json!({"name": rest}),
        });
    }
    if let Some(rest) = strip_prefix_ci(t, "play ") {
        return Some(Routed {
            title: format!("Play “{rest}”"),
            action: "media.play",
            args: json!({"query": rest}),
        });
    }
    if let Some(rest) = strip_prefix_ci(t, "search ") {
        return Some(Routed {
            title: format!("Search the web for “{rest}”"),
            action: "web.search",
            args: json!({"query": rest}),
        });
    }
    if let Some(rest) = strip_prefix_ci(t, "read ") {
        return Some(Routed {
            title: format!("Read {rest}"),
            action: "fs.read",
            args: json!({"path": rest}),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_commands() {
        let r = route("Open Browser").unwrap();
        assert_eq!(
            (r.action, r.args.clone()),
            ("app.launch", json!({"name": "Browser"}))
        );
        assert_eq!(
            route("PLAY lofi beats").unwrap().args,
            json!({"query": "lofi beats"})
        );
        assert_eq!(route("search rust news").unwrap().action, "web.search");
        assert_eq!(
            route("read todo.txt").unwrap().args,
            json!({"path": "todo.txt"})
        );
        let n = route("note: buy milk and eggs").unwrap();
        assert_eq!(
            n.args,
            json!({"title": "buy milk and eggs", "body": "buy milk and eggs"})
        );
    }

    #[test]
    fn anchored_and_not_empty() {
        assert!(route("please open browser").is_none());
        assert!(route("open").is_none());
        assert!(route("open   ").is_none());
        assert!(route("opening hours of the museum").is_none());
        assert!(route("what is the weather").is_none());
        assert!(route("é").is_none());
    }
}
