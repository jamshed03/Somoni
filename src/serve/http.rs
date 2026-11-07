//! The HTTP part of `somoni serve`: reading a request, the status words, content types and writing an answer.

use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;

use crate::builtins::html_safe;

/// An answer to a request.
pub(super) struct Response {
    pub(super) status: u16,
    pub(super) content_type: &'static str,
    pub(super) body: Vec<u8>,
}

/// Reads the request and returns its first line, like `GET /about HTTP/1.1`; the headers after it are skipped.
pub(super) fn read_request(stream: &TcpStream) -> io::Result<String> {
    let mut reader = BufReader::new(stream);
    let mut first_line = String::new();
    reader.read_line(&mut first_line)?;
    let mut header = String::new();
    while reader.read_line(&mut header)? > 0 && header.trim() != "" {
        header.clear();
    }
    Ok(first_line)
}

/// Returns the path of a GET request line, like `/about` for `GET /about HTTP/1.1`.
pub(super) fn request_path(first_line: &str) -> Option<&str> {
    let mut parts = first_line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    (method == "GET").then_some(path)
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
        body: html.into_bytes(),
    }
}

/// Returns the words after the number of a status, like `Not Found` for 404.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    }
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
pub(super) fn write_response(stream: &mut TcpStream, response: &Response) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_get_requests_have_a_path() {
        assert_eq!(request_path("GET /about HTTP/1.1\r\n"), Some("/about"));
        assert_eq!(request_path("POST /about HTTP/1.1\r\n"), None);
        assert_eq!(request_path(""), None);
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
