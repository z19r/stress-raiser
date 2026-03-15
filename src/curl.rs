//! Build HTTP requests from form fields (URL, method, headers, body).

use anyhow::{Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, Method, Request,
};
use std::str::FromStr;

/// HTTP request built from the form (URL, method, headers, body).
#[derive(Debug, Clone)]
pub struct CurlRequest {
    pub url: String,
    pub method: Method,
    pub headers: HeaderMap,
    pub body: Option<Vec<u8>>,
}

impl CurlRequest {
    /// Build from form fields: URL, method, headers text, body text.
    pub fn from_form(url: &str, method: &str, headers_text: &str, body_text: &str) -> Result<Self> {
        let url = url.trim().to_string();
        if url.is_empty() {
            anyhow::bail!("URL is required");
        }
        let method = Method::from_str(method).unwrap_or(Method::GET);
        let mut headers = HeaderMap::new();
        for line in headers_text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let _ = parse_header(line, &mut headers);
        }
        let body = if body_text.trim().is_empty() {
            None
        } else {
            Some(body_text.trim().to_string().into_bytes())
        };
        Ok(Self {
            url,
            method,
            headers,
            body,
        })
    }

    /// Build a reqwest client with 30s timeout.
    pub fn build_client(&self) -> Result<Client> {
        Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("build reqwest client")
    }

    /// Headers as "Name: value" lines for display.
    pub fn headers_display(&self) -> Vec<String> {
        self.headers
            .iter()
            .map(|(k, v)| format!("{}: {}", k.as_str(), v.to_str().unwrap_or("[binary]")))
            .collect()
    }

    /// Body preview (first 500 chars/bytes) for display.
    pub fn body_preview(&self) -> String {
        match &self.body {
            None => "(none)".into(),
            Some(b) => {
                let s = String::from_utf8_lossy(b);
                let trim = s.trim();
                if trim.len() <= 500 {
                    trim.to_string()
                } else {
                    format!("{}… ({} bytes)", &trim[..500], b.len())
                }
            }
        }
    }

    /// Build a reqwest Request from this CurlRequest.
    pub fn build_request(&self, client: &Client) -> Result<Request> {
        let mut req = client
            .request(self.method.clone(), &self.url)
            .headers(self.headers.clone());
        if let Some(ref b) = self.body {
            req = req.body(b.clone());
        }
        req.build().context("build request")
    }
}

fn parse_header(s: &str, headers: &mut HeaderMap) -> Result<()> {
    let Some(colon) = s.find(':') else {
        anyhow::bail!("invalid header: {s}");
    };
    let (name, value) = s.split_at(colon);
    let value = value[1..].trim();
    let name = HeaderName::from_str(name.trim()).context("invalid header name")?;
    let value = HeaderValue::from_str(value).context("invalid header value")?;
    headers.insert(name, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_form_empty_url_err() {
        assert!(CurlRequest::from_form("", "GET", "", "").is_err());
    }

    #[test]
    fn from_form_valid() {
        let r = CurlRequest::from_form("https://x.com", "POST", "A: B\n", "body").unwrap();
        assert_eq!(r.url, "https://x.com");
        assert_eq!(r.method.as_str(), "POST");
        assert_eq!(r.body.as_deref(), Some(b"body".as_slice()));
    }

    #[test]
    fn from_form_plain_url_get() {
        let r = CurlRequest::from_form("https://example.com", "GET", "", "").unwrap();
        assert_eq!(r.url, "https://example.com");
        assert_eq!(r.method.as_str(), "GET");
        assert!(r.body.is_none());
    }
}
