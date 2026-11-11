//! The HTTP part of `somoni serve`: reading a request, the status words, content types and writing an answer.

use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::builtins::html_safe;

/// An answer to a request.
pub(super) struct Response {
    pub(super) status: u16,
    pub(super) content_type: &'static str,
    /// The header `Location` of a redirect, like `/guestbook`.
    pub(super) location: Option<String>,
    /// The headers a page set with `set_header`; a `Content-Type` among them replaces `content_type`.
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: Vec<u8>,
}

/// The most bytes the body of a request may have, like the fields of a form.
const MAX_BODY: usize = 1 << 20;

/// A request as it came in.
pub(super) struct Incoming {
    /// The first line, like `POST /guestbook HTTP/1.1`, for the log.
    pub(super) line: String,
    pub(super) method: String,
    /// The path with its query, like `/search?q=a`, still encoded.
    pub(super) target: String,
    /// The header `Content-Type`, like `application/x-www-form-urlencoded`.
    pub(super) content_type: String,
    pub(super) body: Vec<u8>,
}

/// Reads a request: its first line, the headers `Content-Length` and `Content-Type`, and its body;
/// None if the first line has no method and target or the body is bigger than 1 MiB.
pub(super) fn read_request(reader: &mut impl BufRead) -> io::Result<Option<Incoming>> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut length = 0;
    let mut content_type = String::new();
    let mut header = String::new();
    while reader.read_line(&mut header)? > 0 && !header.trim().is_empty() {
        if let Some((name, value)) = header.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().unwrap_or(0),
                "content-type" => content_type = value.trim().to_string(),
                _ => {}
            }
        }
        header.clear();
    }
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };
    if length > MAX_BODY {
        return Ok(None);
    }
    let (method, target) = (method.to_string(), target.to_string());
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(Incoming {
        line: line.trim().to_string(),
        method,
        target,
        content_type,
        body,
    }))
}

/// Returns a small HTML page for `status` that shows `message`.
pub(super) fn page(status: u16, message: &str) -> Response {
    let title = format!("{} {}", status, reason(status));
    let html = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\"><title>{}</title></head>\n<body>\n<h1>{}</h1>\n<pre>{}</pre>\n</body>\n</html>\n",
        title,
        title,
        html_safe(message)
    );
    Response {
        status,
        content_type: "text/html; charset=utf-8",
        location: None,
        headers: Vec::new(),
        body: html.into_bytes(),
    }
}

/// Returns the words after the number of a status, like `Not Found` for 404, or the words of its class.
fn reason(status: u16) -> &'static str {
    known_reason(status).unwrap_or(match status {
        100..=199 => "Informational",
        200..=299 => "Success",
        300..=399 => "Redirection",
        400..=499 => "Client Error",
        _ => "Server Error",
    })
}

/// Returns the words of a status that has its own, like `Not Found` for 404.
fn known_reason(status: u16) -> Option<&'static str> {
    let words = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        410 => "Gone",
        413 => "Content Too Large",
        422 => "Unprocessable Content",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => return None,
    };
    Some(words)
}

/// Returns the type of a file from its ending, which the browser needs to show it.
pub(super) fn content_type(path: &Path) -> &'static str {
    let ending = path
        .extension()
        .and_then(|ending| ending.to_str())
        .unwrap_or_default();
    match ending.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// Writes the status line, the headers and the body of `response`.
pub(super) fn write_response(out: &mut impl Write, response: &Response) -> io::Result<()> {
    let location = response
        .location
        .as_ref()
        .map(|location| format!("Location: {}\r\n", location))
        .unwrap_or_default();
    let own_type =
        (response.headers.iter()).any(|(name, _)| name.eq_ignore_ascii_case("content-type"));
    let content_type = if own_type {
        String::new()
    } else {
        format!("Content-Type: {}\r\n", response.content_type)
    };
    let headers: String = (response.headers.iter())
        .map(|(name, value)| format!("{}: {}\r\n", name, value))
        .collect();
    let head = format!(
        "HTTP/1.1 {} {}\r\n{}{}Content-Length: {}\r\n{}Connection: close\r\n\r\n",
        response.status,
        reason(response.status),
        content_type,
        headers,
        response.body.len(),
        location
    );
    out.write_all(head.as_bytes())?;
    out.write_all(&response.body)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the request read from `text`.
    fn read(text: &str) -> Option<Incoming> {
        read_request(&mut io::Cursor::new(text.as_bytes())).unwrap()
    }

    #[test]
    fn a_get_request_has_method_and_target() {
        let request = read("GET /search?q=a HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        assert_eq!(
            (request.method.as_str(), request.target.as_str()),
            ("GET", "/search?q=a")
        );
        assert!(request.body.is_empty());
    }

    #[test]
    fn a_post_request_has_its_body() {
        let text = "POST /guestbook HTTP/1.1\r\nContent-Type: application/x-www-form-urlencoded\r\ncontent-length: 7\r\n\r\nname=Al";
        let request = read(text).unwrap();
        assert_eq!(request.content_type, "application/x-www-form-urlencoded");
        assert_eq!(request.body, b"name=Al");
    }

    #[test]
    fn a_broken_or_too_big_request_is_none() {
        assert!(read("\r\n").is_none());
        assert!(read("POST / HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n").is_none());
    }

    #[test]
    fn a_redirect_has_its_location_header() {
        let response = Response {
            status: 303,
            content_type: "text/html; charset=utf-8",
            location: Some("/guestbook".to_string()),
            headers: Vec::new(),
            body: b"x".to_vec(),
        };
        let mut written = Vec::new();
        write_response(&mut written, &response).unwrap();
        let text = String::from_utf8(written).unwrap();
        assert!(text.starts_with("HTTP/1.1 303 See Other\r\n"), "{}", text);
        assert!(text.contains("\r\nLocation: /guestbook\r\n"), "{}", text);
        assert!(text.ends_with("\r\n\r\nx"), "{}", text);
    }

    #[test]
    fn statuses_have_their_words_or_those_of_their_class() {
        assert_eq!(reason(422), "Unprocessable Content");
        assert_eq!(reason(299), "Success");
        assert_eq!(reason(599), "Server Error");
    }

    #[test]
    fn content_types_come_from_the_ending() {
        assert_eq!(content_type(Path::new("a/b.SVG")), "image/svg+xml");
        assert_eq!(
            content_type(Path::new("page.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type(Path::new("data.bin")),
            "application/octet-stream"
        );
    }
}
