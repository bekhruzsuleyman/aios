//! `web.search` and `media.play` helpers. Nothing is scraped: results come
//! from a JSON search API, or the browser is opened on a search page.

use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub title: String,
    pub url: String,
}

fn encode(q: &str) -> String {
    url::form_urlencoded::byte_serialize(q.as_bytes()).collect()
}

pub fn duckduckgo_url(query: &str) -> String {
    format!("https://duckduckgo.com/?q={}", encode(query))
}

pub fn youtube_url(query: &str) -> String {
    format!(
        "https://www.youtube.com/results?search_query={}",
        encode(query)
    )
}

#[derive(Deserialize)]
struct SearxResults {
    #[serde(default)]
    results: Vec<SearxHit>,
}

#[derive(Deserialize)]
struct SearxHit {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
}

const MAX_HITS: usize = 10;
const MAX_FIELD: usize = 300;

fn clip(s: &str) -> String {
    let s: String = s.chars().filter(|c| !c.is_control()).collect();
    s.chars().take(MAX_FIELD).collect()
}

/// Query a SearXNG-compatible endpoint (`<url>?q=...&format=json`).
pub async fn http_json(
    client: &reqwest::Client,
    url: &str,
    query: &str,
) -> Result<Vec<Hit>, String> {
    let mut u = url::Url::parse(url).map_err(|e| format!("search url: {e}"))?;
    u.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("format", "json");
    let resp = client
        .get(u)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("search request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("search provider answered {}", resp.status()));
    }
    let body: SearxResults = resp
        .json()
        .await
        .map_err(|e| format!("search provider sent bad JSON: {e}"))?;
    Ok(body
        .results
        .into_iter()
        .filter(|h| h.url.starts_with("http://") || h.url.starts_with("https://"))
        .take(MAX_HITS)
        .map(|h| Hit {
            title: clip(&h.title),
            url: clip(&h.url),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn urls_are_encoded() {
        assert_eq!(
            duckduckgo_url("rust & c++"),
            "https://duckduckgo.com/?q=rust+%26+c%2B%2B"
        );
        assert_eq!(
            youtube_url("lofi for late night coding"),
            "https://www.youtube.com/results?search_query=lofi+for+late+night+coding"
        );
    }

    #[tokio::test]
    async fn parses_searx_json() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            assert!(
                req.contains("q=rust+news") || req.contains("q=rust%20news"),
                "{req}"
            );
            let body = r#"{"results":[{"title":"Rust 2.0","url":"https://blog.rust-lang.org/"},{"title":"bad","url":"javascript:x"}]}"#;
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
            s.write_all(resp.as_bytes()).await.unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let hits = http_json(&client, &format!("http://{addr}/search"), "rust news")
            .await
            .unwrap();
        assert_eq!(
            hits,
            vec![Hit {
                title: "Rust 2.0".into(),
                url: "https://blog.rust-lang.org/".into()
            }]
        );
    }
}
