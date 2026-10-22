//! Real system actions. Only the broker calls `execute`; everything here
//! takes typed arguments, and nothing builds a command line from text.

pub mod fsread;
pub mod launch;
pub mod notes;
pub mod search;

use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

/// The built-in actions with their typed arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    AppLaunch { app: launch::App },
    WebSearch { query: String },
    MediaPlay { query: String },
    NotesCreate { title: String, body: String },
    FsRead { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Unknown(String),
    BadArgs(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Unknown(a) => write!(f, "unknown action {a}"),
            ParseError::BadArgs(e) => write!(f, "bad arguments: {e}"),
        }
    }
}

const MAX_TEXT: usize = 4096;

fn args<'de, T: Deserialize<'de>>(v: &'de Value) -> Result<T, ParseError> {
    T::deserialize(v).map_err(|e| ParseError::BadArgs(e.to_string()))
}

fn text(s: String, what: &str) -> Result<String, ParseError> {
    let s = s.trim().to_string();
    if s.is_empty() {
        return Err(ParseError::BadArgs(format!("{what} is empty")));
    }
    if s.len() > MAX_TEXT {
        return Err(ParseError::BadArgs(format!("{what} is too long")));
    }
    Ok(s)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameArgs {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArgs {
    query: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteArgs {
    title: String,
    #[serde(default)]
    body: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArgs {
    path: String,
}

impl Action {
    /// Parse an action name and JSON args into a typed action.
    pub fn parse(name: &str, v: &Value) -> Result<Action, ParseError> {
        match name {
            "app.launch" => {
                let a: NameArgs = args(v)?;
                let app = launch::App::from_name(&a.name)
                    .ok_or_else(|| ParseError::BadArgs(format!("no app called {:?}", a.name)))?;
                Ok(Action::AppLaunch { app })
            }
            "web.search" => Ok(Action::WebSearch {
                query: text(args::<QueryArgs>(v)?.query, "query")?,
            }),
            "media.play" => Ok(Action::MediaPlay {
                query: text(args::<QueryArgs>(v)?.query, "query")?,
            }),
            "notes.create" => {
                let a: NoteArgs = args(v)?;
                if a.body.len() > 64 * 1024 {
                    return Err(ParseError::BadArgs("note body is too long".into()));
                }
                Ok(Action::NotesCreate {
                    title: text(a.title, "title")?,
                    body: a.body,
                })
            }
            "fs.read" => Ok(Action::FsRead {
                path: PathBuf::from(text(args::<PathArgs>(v)?.path, "path")?),
            }),
            other => Err(ParseError::Unknown(other.to_string())),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Action::AppLaunch { .. } => "app.launch",
            Action::WebSearch { .. } => "web.search",
            Action::MediaPlay { .. } => "media.play",
            Action::NotesCreate { .. } => "notes.create",
            Action::FsRead { .. } => "fs.read",
        }
    }
}

/// Result of an action. `content` is data from outside (web, files) and is
/// always untrusted: it may be shown or given to the model as data, never
/// interpreted as instructions.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub summary: String,
    pub content: Option<String>,
    pub untrusted: bool,
}

impl Output {
    fn plain(summary: impl Into<String>) -> Output {
        Output {
            summary: summary.into(),
            content: None,
            untrusted: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SearchProvider {
    /// Open the browser on a DuckDuckGo search page.
    BrowserFallback,
    /// GET a SearXNG-compatible JSON endpoint.
    HttpJson { url: String },
}

/// What actions need from the system.
#[derive(Debug, Clone)]
pub struct Context {
    pub notes_dir: PathBuf,
    pub read_roots: Vec<PathBuf>,
    pub launch_socket: PathBuf,
    pub search: SearchProvider,
    pub http: reqwest::Client,
}

pub async fn execute(ctx: &Context, action: &Action) -> Result<Output, String> {
    match action {
        Action::AppLaunch { app } => {
            launch::request(&ctx.launch_socket, *app, None).await?;
            Ok(Output::plain(format!("Opened {}", app.label())))
        }
        Action::WebSearch { query } => match &ctx.search {
            SearchProvider::BrowserFallback => {
                let url = search::duckduckgo_url(query);
                launch::request(&ctx.launch_socket, launch::App::Browser, Some(&url)).await?;
                Ok(Output::plain(format!(
                    "Searching the web for “{query}” in the browser"
                )))
            }
            SearchProvider::HttpJson { url } => {
                let hits = search::http_json(&ctx.http, url, query).await?;
                let summary = format!("{} results for “{query}”", hits.len());
                let content = hits
                    .iter()
                    .map(|h| format!("{} <{}>", h.title, h.url))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(Output {
                    summary,
                    content: Some(content),
                    untrusted: true,
                })
            }
        },
        Action::MediaPlay { query } => {
            let url = search::youtube_url(query);
            launch::request(&ctx.launch_socket, launch::App::Browser, Some(&url)).await?;
            Ok(Output::plain(format!(
                "Playing “{query}” on YouTube in the browser"
            )))
        }
        Action::NotesCreate { title, body } => {
            let path = notes::create(&ctx.notes_dir, title, body)?;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(Output::plain(format!("Saved note {name}")))
        }
        Action::FsRead { path } => {
            let text = fsread::read(&ctx.read_roots, path)?;
            Ok(Output {
                summary: format!("Read {} ({} bytes)", path.display(), text.len()),
                content: Some(text),
                untrusted: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_typed_args() {
        assert_eq!(
            Action::parse("app.launch", &json!({"name": "Browser"})).unwrap(),
            Action::AppLaunch {
                app: launch::App::Browser
            }
        );
        assert!(matches!(
            Action::parse("app.launch", &json!({"name": "rm -rf /"})),
            Err(ParseError::BadArgs(_))
        ));
        assert!(matches!(
            Action::parse("web.search", &json!({"query": "x", "cmd": "y"})),
            Err(ParseError::BadArgs(_))
        ));
        assert!(matches!(
            Action::parse("web.search", &json!({"query": "  "})),
            Err(ParseError::BadArgs(_))
        ));
        assert!(matches!(
            Action::parse("shell.exec", &json!({"cmd": "id"})),
            Err(ParseError::Unknown(_))
        ));
        assert_eq!(
            Action::parse("notes.create", &json!({"title": "milk"})).unwrap(),
            Action::NotesCreate {
                title: "milk".into(),
                body: String::new()
            }
        );
    }
}
