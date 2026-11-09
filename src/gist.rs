use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde_json::{Value, json};
use std::{collections::BTreeMap, process::Command, thread::sleep, time::Duration};

const API: &str = "https://api.github.com/gists";

pub struct Gist {
    client: Client,
    token: Option<String>,
}

impl Gist {
    pub fn new(authenticated: bool) -> Result<Self> {
        let client = Client::builder().user_agent("fog").timeout(None).build()?;
        let token = if authenticated { Some(token()?) } else { None };
        Ok(Self { client, token })
    }

    /// Creates a secret gist; returns (id, html_url).
    pub fn create(&self, files: &[(&str, &str)]) -> Result<(String, String)> {
        let body = json!({ "public": false, "description": "fog", "files": files_json(files) });
        let v: Value = self.send(|| self.client.post(API).json(&body))?.json()?;
        Ok((str_field(&v, "id")?, str_field(&v, "html_url")?))
    }

    pub fn add(&self, id: &str, files: &[(&str, &str)]) -> Result<()> {
        let body = json!({ "files": files_json(files) });
        self.send(|| self.client.patch(format!("{API}/{id}")).json(&body))?;
        Ok(())
    }

    /// Returns file name -> raw_url.
    pub fn files(&self, id: &str) -> Result<BTreeMap<String, String>> {
        let v: Value = self
            .send(|| self.client.get(format!("{API}/{id}")))?
            .json()?;
        if v["truncated"].as_bool() == Some(true) {
            bail!("gist file list truncated (more than 300 files)");
        }
        let files = v["files"].as_object().context("gist has no files")?;
        files
            .iter()
            .map(|(name, f)| Ok((name.clone(), str_field(f, "raw_url")?)))
            .collect()
    }

    pub fn raw(&self, url: &str) -> Result<Vec<u8>> {
        Ok(self.send(|| self.client.get(url))?.bytes()?.to_vec())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.send(|| self.client.delete(format!("{API}/{id}")))?;
        Ok(())
    }

    /// Sends with auth; retries on rate limits (403/429) up to 5 times.
    fn send(&self, req: impl Fn() -> RequestBuilder) -> Result<Response> {
        for attempt in 1.. {
            let mut r = req().header("Accept", "application/vnd.github+json");
            if let Some(t) = &self.token {
                r = r.bearer_auth(t);
            }
            let resp = r.send()?;
            let s = resp.status();
            if s.is_success() {
                return Ok(resp);
            }
            let limited = s.as_u16() == 429
                || (s.as_u16() == 403 && resp.headers().contains_key("retry-after"))
                || resp
                    .headers()
                    .get("x-ratelimit-remaining")
                    .is_some_and(|v| v == "0");
            if !limited || attempt >= 5 {
                bail!("GitHub API {s}: {}", resp.text().unwrap_or_default());
            }
            let wait = header_u64(&resp, "retry-after").unwrap_or(60);
            eprintln!("rate limited, waiting {wait}s");
            sleep(Duration::from_secs(wait));
        }
        unreachable!()
    }
}

fn token() -> Result<String> {
    if let Ok(t) = std::env::var("GITHUB_TOKEN")
        && !t.is_empty()
    {
        return Ok(t);
    }
    let out = Command::new("gh")
        .args(["auth", "token"])
        .output()
        .context("no GITHUB_TOKEN and `gh` not found")?;
    if !out.status.success() {
        bail!("`gh auth token` failed; run `gh auth login`");
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

fn files_json(files: &[(&str, &str)]) -> Value {
    files
        .iter()
        .map(|(n, c)| (n.to_string(), json!({ "content": c })))
        .collect()
}

fn str_field(v: &Value, k: &str) -> Result<String> {
    Ok(v[k]
        .as_str()
        .with_context(|| format!("missing `{k}` in response"))?
        .to_string())
}

fn header_u64(r: &Response, k: &str) -> Option<u64> {
    r.headers().get(k)?.to_str().ok()?.parse().ok()
}

/// Accepts a gist id or any gist URL.
pub fn parse_id(s: &str) -> &str {
    s.trim_end_matches('/').rsplit('/').next().unwrap_or(s)
}
