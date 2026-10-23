//! The `ilmekapp://` content scheme: serves a window's own verified package and nothing else.
#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects command arguments and scheme-handler parameters by value"
)]

use std::borrow::Cow;

use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, Wry};

use crate::state::Shell;

pub(crate) const SCHEME: &str = "ilmekapp";

fn refuse(status: StatusCode) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header(
            "Content-Security-Policy",
            ilmek_runtime::CONTENT_SECURITY_POLICY,
        )
        .body(Cow::Borrowed(&b"forbidden"[..]))
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(&b""[..])))
}

/// A request is served only if the window that made it owns a session whose private host name
/// matches the URL. Another window, or another application's host name, gets 403.
pub(crate) fn handle(
    ctx: UriSchemeContext<'_, Wry>,
    request: Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let shell = ctx.app_handle().state::<Shell>();
    let Some(session) = shell.session(ctx.webview_label()) else {
        tracing::debug!(label = ctx.webview_label(), uri = %request.uri(), "no session for this window");
        return refuse(StatusCode::FORBIDDEN);
    };
    let uri = request.uri();
    if uri.host() != Some(session.host_name().as_str()) {
        tracing::debug!(uri = %uri, "request for another application's host name");
        return refuse(StatusCode::FORBIDDEN);
    }
    let target = uri
        .path_and_query()
        .map_or("/", tauri::http::uri::PathAndQuery::as_str);
    let asset = session.serve(target);
    tracing::debug!(
        path = target,
        status = asset.status,
        "served application content"
    );

    let mut response = Response::builder()
        .status(StatusCode::from_u16(asset.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
        .header("Content-Type", asset.content_type);
    for (name, value) in asset.headers {
        response = response.header(name, value);
    }
    response
        .body(Cow::Owned(asset.body))
        .unwrap_or_else(|_| refuse(StatusCode::INTERNAL_SERVER_ERROR))
}
