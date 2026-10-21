//! Importing from an API docs page. The page is fetched, then its HTML is
//! searched for an OpenAPI spec: a `spec-url` attribute (Redoc), a `url:`
//! setting in a script (Swagger UI), or a link to a `.json` or `.yaml`
//! file that names a spec. The first spec that parses is imported. A page
//! that is itself a spec is imported directly.

use std::time::Duration;

use scraper::{Html, Selector};
use url::Url;

use super::{Imported, parse_file};
use crate::http;

/// Where the spec might be, in the order they should be tried. Each URL is
/// resolved against `page`; only http and https URLs are kept, once each.
pub fn spec_urls(html: &str, page: &Url) -> Vec<Url> {
    let doc = Html::parse_document(html);
    let mut found: Vec<String> = Vec::new();
    // Redoc: <redoc spec-url="…">. The attribute is the spec, so no name check.
    for el in doc.select(&selector("[spec-url]")) {
        found.extend(el.value().attr("spec-url").map(str::to_string));
    }
    // Swagger UI: `url: "…"` in its inline configuration script.
    for script in doc.select(&selector("script")) {
        let text: String = script.text().collect();
        found.extend(config_urls(&text));
    }
    // Links to spec files, if the file name says what it is.
    for el in doc.select(&selector("a[href], link[href]")) {
        if let Some(href) = el.value().attr("href")
            && is_spec_file(href)
            && names_a_spec(href)
        {
            found.push(href.to_string());
        }
    }
    let mut out: Vec<Url> = Vec::new();
    for raw in found {
        if let Ok(url) = page.join(&raw)
            && matches!(url.scheme(), "http" | "https")
            && !out.contains(&url)
        {
            out.push(url);
        }
    }
    out
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("the selectors here are fixed and valid")
}

/// True for a path that ends in a spec file type. A query or fragment does
/// not count.
fn is_spec_file(href: &str) -> bool {
    let path = href
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    [".json", ".yaml", ".yml"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

/// Spec file names usually say so. Used for plain links, where a `.json`
/// may be anything.
fn names_a_spec(href: &str) -> bool {
    let lower = href.to_ascii_lowercase();
    ["openapi", "swagger", "spec"]
        .iter()
        .any(|word| lower.contains(word))
}

/// The spec files a script names in quoted strings, on one line each. A
/// string counts when its name says it is a spec (`const specUrl = "…/openapi.json"`),
/// or when it is the value of a `url` key (`url: "./openapi.yaml"`, the
/// Swagger UI form). A variable as the value gives nothing: the URL is then
/// computed in code, which this search does not run.
fn config_urls(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = script;
    while let Some(start) = rest.find(['"', '\'']) {
        let quote = rest.as_bytes()[start] as char;
        let inner = &rest[start + 1..];
        let line = &inner[..inner.find('\n').unwrap_or(inner.len())];
        let Some(end) = line.find(quote) else {
            rest = inner;
            continue;
        };
        let candidate = &line[..end];
        if is_spec_file(candidate) && (names_a_spec(candidate) || is_url_value(&rest[..start])) {
            out.push(candidate.to_string());
        }
        rest = &inner[end + 1..];
    }
    out
}

/// True when the text before a quoted value ends with a `url` key, as in
/// `url:` or `"url":`. `baseurl:` does not count.
fn is_url_value(before: &str) -> bool {
    let head = before
        .trim_end()
        .trim_end_matches(':')
        .trim_end()
        .trim_end_matches(['"', '\'']);
    head.strip_suffix("url")
        .is_some_and(|rest| !rest.ends_with(|c: char| c.is_alphanumeric() || c == '_'))
}

/// Fetches `url` and imports the API it describes. The URL may be a docs
/// page with a spec link, or the spec itself.
pub fn fetch(url: &str, timeout: Duration) -> Result<Imported, String> {
    let body = http::get_text(url, timeout)?;
    // The URL may be the spec itself.
    if let Ok(imported) = parse_file(&body) {
        return Ok(imported);
    }
    let page = Url::parse(url).map_err(|e| format!("Not a URL: {e}"))?;
    // The first candidate that fails says why, which is more useful than a
    // generic message (for example, a Swagger 2.0 spec is refused).
    let mut why = None;
    for candidate in spec_urls(&body, &page) {
        let text = match http::get_text(candidate.as_str(), timeout) {
            Ok(text) => text,
            Err(e) => {
                why.get_or_insert(e);
                continue;
            }
        };
        match parse_file(&text) {
            Ok(imported) => return Ok(imported),
            Err(e) => {
                why.get_or_insert(format!("{candidate}: {e}"));
            }
        }
    }
    let reason = why.map_or_else(String::new, |w| format!(" ({w})"));
    Err(format!(
        "No OpenAPI spec found at {url}{reason}. Paste the spec's own URL instead, or check the page for a spec link."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    fn page(url: &str) -> Url {
        Url::parse(url).unwrap()
    }

    fn strings(urls: &[Url]) -> Vec<String> {
        urls.iter().map(Url::to_string).collect()
    }

    #[test]
    fn redoc_names_its_spec_in_an_attribute() {
        let html = include_str!("../../fixtures/import/docs/redoc.html");
        let urls = spec_urls(html, &page("https://docs.example.test/reference"));
        assert_eq!(
            strings(&urls),
            vec!["https://docs.example.test/spec/orders.json"]
        );
    }

    #[test]
    fn swagger_ui_names_its_spec_in_a_script_and_resolves_it() {
        let html = include_str!("../../fixtures/import/docs/swagger-ui.html");
        let urls = spec_urls(html, &page("https://docs.example.test/api/index.html"));
        assert_eq!(
            strings(&urls),
            vec!["https://docs.example.test/api/openapi.yaml"]
        );
    }

    #[test]
    fn spec_links_are_found_once_and_resolved_against_the_page() {
        let html = include_str!("../../fixtures/import/docs/plain-links.html");
        let urls = spec_urls(html, &page("https://docs.example.test/guides/"));
        assert_eq!(
            strings(&urls),
            vec!["https://docs.example.test/downloads/openapi.json"],
            "the guide link, the logo and the mailto are not specs"
        );
    }

    #[test]
    fn a_spec_named_in_a_script_counts_without_a_url_key() {
        let html = r#"<script>
            const specUrl = "https://api.example.test/docs/openapi.json";
            const validator = "https://validator.example.test/check";
        </script>"#;
        let urls = spec_urls(html, &page("https://docs.example.test/"));
        assert_eq!(
            strings(&urls),
            vec!["https://api.example.test/docs/openapi.json"],
            "a quoted string that is not a spec file is not a candidate"
        );
    }

    #[test]
    fn a_url_key_set_to_a_variable_gives_nothing() {
        let html = r##"<script>
            SwaggerUIBundle({ url: definitionURL, dom_id: "#swagger-ui" });
        </script>"##;
        assert!(spec_urls(html, &page("https://docs.example.test/")).is_empty());
    }

    #[test]
    fn a_page_without_a_spec_has_no_candidates() {
        let html = include_str!("../../fixtures/import/docs/nothing-here.html");
        assert!(spec_urls(html, &page("https://docs.example.test/")).is_empty());
    }

    /// A tiny server: `routes` maps a request path to (content type, body).
    /// It answers any number of requests and stops with the test.
    fn serve(routes: Vec<(&'static str, &'static str, &'static str)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let path = request_line.split_whitespace().nth(1).unwrap_or("/");
                // Drain the headers so the client sees a clean close.
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let (status, kind, body) = match routes.iter().find(|(p, _, _)| *p == path) {
                    Some((_, kind, body)) => ("200 OK", *kind, *body),
                    None => ("404 Not Found", "text/plain", "missing"),
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        base
    }

    const SPEC: &str = r#"{
        "openapi": "3.0.0",
        "info": {"title": "Orders", "version": "1"},
        "paths": {"/orders": {"get": {"responses": {"200": {"description": "ok"}}}}}
    }"#;

    #[test]
    fn fetch_follows_a_spec_link_on_the_page() {
        let base = serve(vec![
            ("/docs", "text/html", r#"<a href="/spec.json">OpenAPI</a>"#),
            ("/spec.json", "application/json", SPEC),
        ]);
        let imported = fetch(&format!("{base}/docs"), Duration::from_secs(5)).unwrap();
        assert_eq!(imported.name, "Orders");
        assert_eq!(imported.items[0].name, "GET /orders");
    }

    #[test]
    fn fetch_takes_a_spec_url_directly() {
        let base = serve(vec![("/spec.json", "application/json", SPEC)]);
        let imported = fetch(&format!("{base}/spec.json"), Duration::from_secs(5)).unwrap();
        assert_eq!(imported.name, "Orders");
    }

    #[test]
    #[ignore = "needs the internet"]
    fn live_https_spec_imports() {
        let url = "https://petstore3.swagger.io/api/v3/openapi.json";
        let imported = fetch(url, Duration::from_secs(30)).unwrap();
        assert!(!imported.items.is_empty());
    }

    #[test]
    fn fetch_reports_why_the_spec_it_found_was_refused() {
        let swagger2 =
            r#"{"swagger": "2.0", "info": {"title": "Old", "version": "1"}, "paths": {}}"#;
        let base = serve(vec![
            (
                "/docs",
                "text/html",
                r#"<script>fetch("/spec.json")</script>"#,
            ),
            ("/spec.json", "application/json", swagger2),
        ]);
        let err = fetch(&format!("{base}/docs"), Duration::from_secs(5)).unwrap_err();
        assert!(err.contains("Swagger 2.0"), "{err}");
    }

    #[test]
    fn fetch_without_a_spec_says_so() {
        let base = serve(vec![("/blog", "text/html", "<p>No spec here.</p>")]);
        let err = fetch(&format!("{base}/blog"), Duration::from_secs(5)).unwrap_err();
        assert!(err.contains("No OpenAPI spec"), "{err}");
    }
}
