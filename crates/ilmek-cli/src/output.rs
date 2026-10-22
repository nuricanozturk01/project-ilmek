use std::io::Write;

/// Result data goes to stdout so it can be piped; diagnostics go to stderr.
pub(crate) fn line(text: &str) {
    let _ = writeln!(std::io::stdout(), "{text}");
}

pub(crate) fn note(text: &str) {
    let _ = writeln!(std::io::stderr(), "{text}");
}

pub(crate) fn error(text: &str) {
    let _ = writeln!(std::io::stderr(), "error: {text}");
}
