//! A resolved request as a `curl` command line. Pure: it formats exactly
//! what `http.rs` would send, so pasting it into a shell sends the same
//! request.

use crate::resolve::Resolved;

/// One argument per line after the URL, joined with ` \` so the command
/// pastes into a shell and stays readable.
pub fn command(req: &Resolved) -> String {
    let mut args: Vec<String> = Vec::new();
    match req.method.as_str() {
        // curl sends GET by default.
        "GET" => {}
        // `-X HEAD` makes curl wait for a body that never comes.
        "HEAD" => args.push("--head".into()),
        method => args.push(format!("-X {method}")),
    }
    for (name, value) in &req.headers {
        args.push(format!("-H {}", quote(&format!("{name}: {value}"))));
    }
    if !req.body.is_empty() {
        args.push(format!(
            "--data-binary {}",
            quote(&String::from_utf8_lossy(&req.body))
        ));
    }
    let mut out = format!("curl {}", quote(&req.url));
    for arg in args {
        out.push_str(" \\\n  ");
        out.push_str(&arg);
    }
    out
}

/// Single-quotes `s` for a POSIX shell. Inside single quotes nothing is
/// special except the quote itself, written as `'\''`.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: &str, url: &str) -> Resolved {
        Resolved {
            method: method.into(),
            url: url.into(),
            headers: vec![],
            body: vec![],
        }
    }

    #[test]
    fn plain_get_is_just_the_url() {
        assert_eq!(
            command(&req("GET", "https://api.test/users?page=2")),
            "curl 'https://api.test/users?page=2'"
        );
    }

    #[test]
    fn post_with_headers_and_body_one_argument_per_line() {
        let r = Resolved {
            headers: vec![
                ("Authorization".into(), "Bearer t0k".into()),
                ("Content-Type".into(), "application/json".into()),
            ],
            body: br#"{"name":"bee"}"#.to_vec(),
            ..req("POST", "https://api.test/users")
        };
        assert_eq!(
            command(&r),
            "curl 'https://api.test/users' \\\n  \
             -X POST \\\n  \
             -H 'Authorization: Bearer t0k' \\\n  \
             -H 'Content-Type: application/json' \\\n  \
             --data-binary '{\"name\":\"bee\"}'"
        );
    }

    #[test]
    fn single_quotes_are_escaped_for_the_shell() {
        let r = Resolved {
            body: b"it's".to_vec(),
            ..req("PUT", "https://x")
        };
        assert!(command(&r).ends_with(r"--data-binary 'it'\''s'"));
    }

    #[test]
    fn head_uses_the_head_flag() {
        assert_eq!(
            command(&req("HEAD", "https://x")),
            "curl 'https://x' \\\n  --head"
        );
    }
}
