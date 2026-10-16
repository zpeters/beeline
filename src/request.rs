//! Build and send one HTTP request. No UI types live here.

pub const METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub body: String,
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

/// Turn the UI's request into an `http::Request`. Pure, so it is tested
/// without touching the network.
pub fn build(req: &Request) -> Result<ureq::http::Request<String>, String> {
    let method =
        ureq::http::Method::from_bytes(req.method.as_bytes()).map_err(|e| e.to_string())?;
    ureq::http::Request::builder()
        .method(method)
        .uri(req.url.trim())
        .body(req.body.clone())
        .map_err(|e| e.to_string())
}

/// Send the request and read the full body. Non-2xx statuses are
/// returned as normal responses, not errors, so the user can see them.
pub fn send(req: &Request) -> Result<Response, String> {
    let built = build(req)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut resp = agent.run(built).map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    Ok(Response { status, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: &str, url: &str, body: &str) -> Request {
        Request {
            method: method.into(),
            url: url.into(),
            body: body.into(),
        }
    }

    #[test]
    fn builds_method_and_trimmed_uri() {
        let built = build(&req("GET", "  https://example.com/x  ", "")).unwrap();
        assert_eq!(built.method(), "GET");
        assert_eq!(built.uri().to_string(), "https://example.com/x");
    }

    #[test]
    fn keeps_body() {
        let built = build(&req("POST", "https://example.com", "{\"a\":1}")).unwrap();
        assert_eq!(built.body(), "{\"a\":1}");
    }

    #[test]
    fn rejects_invalid_method() {
        assert!(build(&req("BAD METHOD", "https://example.com", "")).is_err());
    }
}
