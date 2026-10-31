//! What an application page may load, and under which restrictions (REQUIREMENTS §18.2).

use std::collections::HashMap;
use std::path::PathBuf;

/// Applied to every response. Pages get no network of their own (`connect-src 'none'`) and run
/// in a sandboxed frame of the shell: `sandbox` gives them an opaque origin (no cookies, no web
/// storage, no access to the shell), and `frame-ancestors` lets only the shell embed them. The
/// only way out is `postMessage` to the shell, which the shell routes to this page's own session.
///
/// Resources are allowed by *scheme* (`ilmekapp:`, and the `http://ilmekapp.localhost` form
/// Windows and Android give the same scheme), not by `'self'`: a sandboxed document has an opaque
/// origin, which WebKit also uses for `'self'`, so `'self'` would block the page's own files. The
/// scheme handler serves only open sessions' verified packages.
pub const CONTENT_SECURITY_POLICY: &str = concat!(
    "default-src 'none'; ",
    "script-src ilmekapp: http://ilmekapp.localhost; ",
    "style-src ilmekapp: http://ilmekapp.localhost; ",
    "img-src ilmekapp: http://ilmekapp.localhost data: blob:; ",
    "font-src ilmekapp: http://ilmekapp.localhost; ",
    "media-src ilmekapp: http://ilmekapp.localhost; ",
    "connect-src 'none'; frame-src 'none'; child-src 'none'; object-src 'none'; ",
    "form-action 'none'; base-uri 'none'; ",
    "frame-ancestors tauri://localhost http://tauri.localhost http://localhost:1420; ",
    "sandbox allow-scripts allow-forms",
);

/// Where a page finds the SDK. The shell cannot inject scripts into a frame, so the response
/// for every HTML file of the package loads this script first.
pub const SDK_PATH: &str = "/__ilmek/sdk.js";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

/// The verified files of one package: the only things its page can ever receive.
#[derive(Debug, Clone)]
pub(crate) struct AssetPolicy {
    root: PathBuf,
    files: HashMap<String, u64>,
    entrypoint: String,
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub(crate) fn security_headers() -> Vec<(&'static str, String)> {
    vec![
        (
            "Content-Security-Policy",
            CONTENT_SECURITY_POLICY.to_owned(),
        ),
        ("X-Content-Type-Options", "nosniff".to_owned()),
        ("Cache-Control", "no-store".to_owned()),
        ("Cross-Origin-Resource-Policy", "cross-origin".to_owned()),
        // `<script type="module">` is fetched in CORS mode; the sandboxed frame's opaque origin needs this.
        ("Access-Control-Allow-Origin", "*".to_owned()),
        ("Referrer-Policy", "no-referrer".to_owned()),
        (
            "Permissions-Policy",
            "camera=(), microphone=(), geolocation=(), clipboard-read=()".to_owned(),
        ),
    ]
}

impl AssetPolicy {
    pub(crate) fn new(root: PathBuf, files: HashMap<String, u64>, entrypoint: String) -> Self {
        Self {
            root,
            files,
            entrypoint,
        }
    }

    fn not_found() -> AssetResponse {
        AssetResponse {
            status: 404,
            content_type: "text/plain; charset=utf-8",
            headers: security_headers(),
            body: b"not found".to_vec(),
        }
    }

    /// Resolves a request path such as `/app.js?x=1#top` to a manifest file, or 404.
    /// Anything that is not exactly a listed file is refused; there is no path arithmetic.
    pub(crate) fn serve(&self, request_path: &str) -> AssetResponse {
        let path = request_path.split(['?', '#']).next().unwrap_or_default();
        let relative = match path.strip_prefix('/') {
            Some("") => self.entrypoint.as_str(),
            Some(rest) => rest,
            None => return Self::not_found(),
        };
        let Some(expected_size) = self.files.get(relative) else {
            return Self::not_found();
        };

        let disk = self.root.join("assets").join(relative);
        let regular = std::fs::symlink_metadata(&disk).is_ok_and(|m| m.file_type().is_file());
        let body = if regular {
            std::fs::read(&disk).ok()
        } else {
            None
        };
        match body {
            Some(body) if body.len() as u64 == *expected_size => AssetResponse {
                status: 200,
                content_type: content_type(relative),
                headers: security_headers(),
                body,
            },
            _ => Self::not_found(),
        }
    }

    /// `true` only for same-application pages. Everything else — other schemes, other
    /// applications, external sites, `file:` — is refused and never opened.
    pub(crate) fn allows_navigation(&self, origin_prefix: &str, url: &str) -> bool {
        let Some(rest) = url.strip_prefix(origin_prefix) else {
            return false;
        };
        if !(rest.is_empty()
            || rest.starts_with('/')
            || rest.starts_with('?')
            || rest.starts_with('#'))
        {
            return false;
        }
        let path = rest.split(['?', '#']).next().unwrap_or_default();
        match path.strip_prefix('/') {
            None | Some("") => true,
            Some(file) => self.files.contains_key(file),
        }
    }
}

/// Inserts the SDK script tag as the first thing in `<head>` (or after `<html>`, or first), so
/// `window.ilmek` exists before any script of the page runs.
pub(crate) fn inject_sdk(html: &[u8]) -> Vec<u8> {
    let tag = format!("<script src=\"{SDK_PATH}\"></script>");
    let after = |name: &[u8]| -> Option<usize> {
        let lower = html.to_ascii_lowercase();
        let start = lower
            .windows(name.len())
            .position(|window| window == name)?;
        let boundary = lower.get(start + name.len()).copied();
        if !matches!(boundary, Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/')) {
            return None;
        }
        let end = lower[start..].iter().position(|byte| *byte == b'>')?;
        Some(start + end + 1)
    };
    let at = after(b"<head").or_else(|| after(b"<html")).unwrap_or(0);
    let mut out = Vec::with_capacity(html.len() + tag.len());
    out.extend_from_slice(&html[..at]);
    out.extend_from_slice(tag.as_bytes());
    out.extend_from_slice(&html[at..]);
    out
}
