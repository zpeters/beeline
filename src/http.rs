//! Send a resolved request. The only module that touches the network.

use std::time::{Duration, Instant};

use ureq::http::{Method, Request};

use crate::resolve::Resolved;

const MAX_BODY: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub elapsed: Duration,
}

/// Non-2xx statuses are returned as normal responses so the user sees them.
pub fn send(req: &Resolved, timeout: Duration) -> Result<Response, String> {
    let method = Method::from_bytes(req.method.as_bytes())
        .map_err(|_| format!("Invalid method \"{}\"", req.method))?;
    let mut builder = Request::builder().method(method).uri(&req.url);
    for (k, v) in &req.headers {
        builder = builder.header(k, v);
    }
    let request = builder.body(req.body.clone()).map_err(|e| e.to_string())?;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .timeout_global(Some(timeout))
        .build()
        .into();

    let start = Instant::now();
    let mut resp = agent.run(request).map_err(|e| e.to_string())?;
    let status = resp.status();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = resp
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_vec()
        .map_err(|e| e.to_string())?;

    Ok(Response {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        body,
        elapsed: start.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    /// Serve one canned response; return the raw request the client sent.
    fn serve_once(response: &'static str) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            reader.get_mut().write_all(response.as_bytes()).unwrap();
            head + &String::from_utf8(body).unwrap()
        });
        (url, handle)
    }

    const TIMEOUT: Duration = Duration::from_secs(10);

    #[test]
    fn gives_up_after_the_timeout() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        // Accept the connection, then never answer.
        let server = std::thread::spawn(move || {
            let (_conn, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_secs(3));
        });
        let start = Instant::now();
        let result = send(
            &Resolved {
                method: "GET".into(),
                url,
                headers: vec![],
                body: vec![],
            },
            Duration::from_secs(1),
        );
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
        server.join().unwrap();
    }

    #[test]
    fn sends_headers_and_body_and_reads_response() {
        let (url, server) = serve_once(
            "HTTP/1.1 201 Created\r\nX-Test: yes\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        );
        let resp = send(
            &Resolved {
                method: "POST".into(),
                url: format!("{url}/things"),
                headers: vec![("X-Client".into(), "beeline".into())],
                body: b"{\"a\":1}".to_vec(),
            },
            TIMEOUT,
        )
        .unwrap();
        let seen = server.join().unwrap();

        assert!(seen.starts_with("POST /things HTTP/1.1\r\n"), "{seen}");
        assert!(
            seen.to_ascii_lowercase().contains("x-client: beeline"),
            "{seen}"
        );
        assert!(seen.ends_with("{\"a\":1}"), "{seen}");
        assert_eq!(resp.status, 201);
        assert_eq!(resp.reason, "Created");
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "x-test" && v == "yes")
        );
        assert_eq!(resp.body, b"ok");
    }

    #[test]
    fn error_status_is_a_normal_response() {
        let (url, server) =
            serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let resp = send(
            &Resolved {
                method: "GET".into(),
                url,
                headers: vec![],
                body: vec![],
            },
            TIMEOUT,
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(resp.status, 404);
        assert!(resp.body.is_empty());
    }

    /// Real HTTPS through the demo collection's public API. Opt in with
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs the internet"]
    fn live_https_get() {
        let resp = send(
            &Resolved {
                method: "GET".into(),
                url: "https://jsonplaceholder.typicode.com/posts/1".into(),
                headers: vec![("Accept".into(), "application/json".into())],
                body: vec![],
            },
            TIMEOUT,
        )
        .unwrap();
        assert_eq!(resp.status, 200);
        assert!(String::from_utf8_lossy(&resp.body).contains("\"id\": 1"));
    }
}
