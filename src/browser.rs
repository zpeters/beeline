//! Opens a URL in the default browser, and builds links to a new GitHub
//! issue. The only module that starts another program.

use url::Url;

const NEW_ISSUE: &str = "https://github.com/zpeters/beeline/issues/new";

/// A link to a new issue on beeline's tracker, with the title and body
/// already filled in. Nothing is sent until the user submits it there.
pub fn new_issue_url(title: &str, body: &str) -> String {
    let mut url = Url::parse(NEW_ISSUE).expect("the issue address is fixed and valid");
    url.query_pairs_mut()
        .append_pair("title", title)
        .append_pair("body", body);
    url.into()
}

/// Opens `url` in the default browser.
pub fn open(url: &str) -> Result<(), String> {
    // Each platform's own opener takes the URL as one argument, so the
    // query's `&` needs no shell quoting.
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open a browser: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_issue_link_carries_title_and_body_back_out() {
        let body = "Page: https://docs.example.test/api?x=1&y=2\n\nWhy: no spec";
        let link = new_issue_url("Import failed for docs.example.test", body);
        let parsed = Url::parse(&link).unwrap();
        assert_eq!(parsed.host_str(), Some("github.com"));
        assert_eq!(parsed.path(), "/zpeters/beeline/issues/new");
        let pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
        assert_eq!(
            pairs,
            vec![
                (
                    "title".to_string(),
                    "Import failed for docs.example.test".to_string()
                ),
                ("body".to_string(), body.to_string()),
            ]
        );
    }
}
