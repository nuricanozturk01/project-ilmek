//! What an application page may load, and under which restrictions (REQUIREMENTS §18.2).

use std::collections::HashMap;
use std::path::PathBuf;

/// Applied to every response. Pages get no network of their own: `connect-src` allows only the
/// shell's IPC endpoints, which are further limited by per-window capabilities.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; \
style-src 'self'; img-src 'self' data: blob:; font-src 'self'; media-src 'self'; \
connect-src ipc: http://ipc.localhost; frame-src 'none'; child-src 'none'; object-src 'none'; \
form-action 'none'; base-uri 'none'; frame-ancestors 'none'";

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

fn security_headers() -> Vec<(&'static str, String)> {
    vec![
        (
            "Content-Security-Policy",
            CONTENT_SECURITY_POLICY.to_owned(),
        ),
        ("X-Content-Type-Options", "nosniff".to_owned()),
        ("Cache-Control", "no-store".to_owned()),
        ("Cross-Origin-Resource-Policy", "same-origin".to_owned()),
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
