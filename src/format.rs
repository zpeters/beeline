//! Display helpers for responses. Pure.

use std::time::Duration;

/// Pretty-print a JSON body (keeping key order); otherwise show it as
/// text, replacing invalid UTF-8. Returns the text and whether it was JSON.
pub fn body_text(body: &[u8]) -> (String, bool) {
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body)
        && let Ok(pretty) = serde_json::to_string_pretty(&value)
    {
        return (pretty, true);
    }
    (String::from_utf8_lossy(body).into_owned(), false)
}

/// Cut `text` to at most `max` bytes on a char boundary. Returns whether
/// it was cut.
pub fn truncate(text: &mut String, max: usize) -> bool {
    if text.len() <= max {
        return false;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    true
}

pub fn size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{:.1} MB", b / (KB * KB))
    }
}

pub fn duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.2} s", d.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_prints_json_in_original_key_order() {
        let (text, is_json) = body_text(br#"{"z":1,"a":[true]}"#);
        assert!(is_json);
        assert_eq!(text, "{\n  \"z\": 1,\n  \"a\": [\n    true\n  ]\n}");
    }

    #[test]
    fn non_json_is_plain_text() {
        assert_eq!(body_text(b"<p>hi</p>"), ("<p>hi</p>".to_string(), false));
        assert_eq!(body_text(b""), (String::new(), false));
    }

    #[test]
    fn invalid_utf8_is_replaced() {
        assert_eq!(body_text(&[b'a', 0xff]).0, "a\u{fffd}");
    }

    #[test]
    fn truncates_on_char_boundary() {
        let mut s = "aé".to_string(); // 'é' is 2 bytes
        assert!(truncate(&mut s, 2));
        assert_eq!(s, "a");
        let mut short = "abc".to_string();
        assert!(!truncate(&mut short, 3));
        assert_eq!(short, "abc");
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(3 * 1024 * 1024), "3.0 MB");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration(Duration::from_millis(42)), "42 ms");
        assert_eq!(duration(Duration::from_millis(1234)), "1.23 s");
    }
}
