//! `somoni serve`: a small web server that answers every request with a file of one folder.
//! A Somoni program runs and what it writes is the page; any other file is sent as it is.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use crate::builtins::html_safe;
use crate::{build, render, vm};

/// The endings that make a file a program, tried in this order for a path like `/about`.
const PROGRAM_ENDINGS: &[&str] = &[".smn", ".template.smn"];

/// What a path from the browser stands for.
#[derive(Debug, PartialEq)]
enum Target {
    /// A Somoni program, whose output is the page.
    Program(PathBuf),
    /// Any other file, sent as it is.
    File(PathBuf),
    /// Nothing, or a path that would leave the folder.
    Missing,
}

/// An answer to a request.
struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

/// Answers the requests on localhost:`port` with the files of `dir`, until the user stops it with Ctrl+C.
pub fn serve(dir: &str, port: u16) -> Result<(), String> {
    let root = Path::new(dir)
        .canonicalize()
        .map_err(|err| format!("Cannot serve '{}': {}", dir, err))?;
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|err| format!("Cannot use port {}: {}", port, err))?;
    println!(
        "Serving {} on http://localhost:{} (Ctrl+C stops)",
        root.display(),
        port
    );
    for stream in listener.incoming() {
        let answered = stream.and_then(|stream| answer(stream, &root));
        if let Err(err) = answered {
            eprintln!("request failed: {}", err);
        }
    }
    Ok(())
}

/// Reads one request from `stream`, writes the answer and logs it, like `GET /about 200`.
fn answer(mut stream: TcpStream, root: &Path) -> io::Result<()> {
    let first_line = read_request(&stream)?;
    let response = match request_path(&first_line) {
        Some(path) => respond(root, path),
        None => page(405, "Only GET requests are answered."),
    };
    println!("{} {}", first_line.trim(), response.status);
    write_response(&mut stream, &response)
}

/// Reads the request and returns its first line, like `GET /about HTTP/1.1`; the headers after it are skipped.
fn read_request(stream: &TcpStream) -> io::Result<String> {
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
fn request_path(first_line: &str) -> Option<&str> {
    let mut parts = first_line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    (method == "GET").then_some(path)
}

/// Returns the answer for the path `url`: the output of a program, a file, or a page for 404 or 500.
fn respond(root: &Path, url: &str) -> Response {
    match resolve(root, url) {
        Target::Program(path) => run_page(&path),
        Target::File(path) => match fs::read(&path) {
            Ok(body) => Response {
                status: 200,
                content_type: content_type(&path),
                body,
            },
            Err(err) => page(500, &format!("Cannot read {}: {}", url, err)),
        },
        Target::Missing => page(404, &format!("There is nothing at {}.", url)),
    }
}

/// Finds what `url` stands for in `root`: `/` and `/docs/` look for `index`, `/about` for `about.smn` or
/// `about.template.smn` or a file `about`; `.smn` files are never sent, and no path leaves `root`.
fn resolve(root: &Path, url: &str) -> Target {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let mut relative = PathBuf::new();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        if part == ".." || part == "." {
            return Target::Missing;
        }
        relative.push(part);
    }
    let wants_folder = path.ends_with('/') || relative.as_os_str().is_empty();
    let base = if wants_folder {
        root.join(&relative).join("index")
    } else {
        root.join(&relative)
    };
    let found = PROGRAM_ENDINGS
        .iter()
        .map(|ending| with_ending(&base, ending))
        .find(|program| program.is_file())
        .map(Target::Program)
        .or_else(|| {
            let file = if wants_folder {
                with_ending(&base, ".html")
            } else {
                base.clone()
            };
            (file.is_file() && !file.to_string_lossy().ends_with(".smn"))
                .then_some(Target::File(file))
        });
    match found {
        Some(Target::Program(path)) if inside(root, &path) => Target::Program(path),
        Some(Target::File(path)) if inside(root, &path) => Target::File(path),
        _ => Target::Missing,
    }
}

/// Returns `path` with `ending` added to its last part, like `about.smn` for `about`.
fn with_ending(path: &Path, ending: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(ending);
    PathBuf::from(name)
}

/// Returns true if `path`, with links followed, lies inside `root`.
fn inside(root: &Path, path: &Path) -> bool {
    path.canonicalize().is_ok_and(|real| real.starts_with(root))
}

/// Runs the program `path` and returns what it writes as the page, or a page 500 with its error.
fn run_page(path: &Path) -> Response {
    let built = build(&path.to_string_lossy());
    let output = built.and_then(|(program, files)| {
        let (output, ended) = vm::run_captured(&program);
        ended
            .map(|()| output)
            .map_err(|error| render(&error, &files))
    });
    match output {
        Ok(html) => Response {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: html.into_bytes(),
        },
        Err(message) => page(500, &message),
    }
}

/// Returns a small HTML page for `status` that shows `message`.
fn page(status: u16, message: &str) -> Response {
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
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    }
}

/// Returns the type of a file from its ending, which the browser needs to show it.
fn content_type(path: &Path) -> &'static str {
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
fn write_response(stream: &mut TcpStream, response: &Response) -> io::Result<()> {
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

    /// Makes a fresh folder `name` with a few files to serve and returns its real path; each test has its own,
    /// because tests run at the same time.
    fn site(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("somoni_serve_{}_{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("docs")).unwrap();
        for file in [
            "index.template.smn",
            "about.smn",
            "style.css",
            "docs/index.html",
        ] {
            fs::write(dir.join(file), "x").unwrap();
        }
        dir.canonicalize().unwrap()
    }

    #[test]
    fn paths_find_programs_and_files() {
        let root = site("find");
        assert_eq!(
            resolve(&root, "/"),
            Target::Program(root.join("index.template.smn"))
        );
        assert_eq!(
            resolve(&root, "/about?x=1"),
            Target::Program(root.join("about.smn"))
        );
        assert_eq!(
            resolve(&root, "/style.css"),
            Target::File(root.join("style.css"))
        );
        assert_eq!(
            resolve(&root, "/docs/"),
            Target::File(root.join("docs/index.html"))
        );
        assert_eq!(resolve(&root, "/nothing"), Target::Missing);
    }

    #[test]
    fn source_files_and_paths_out_of_the_folder_are_missing() {
        let root = site("outside");
        assert_eq!(resolve(&root, "/about.smn"), Target::Missing);
        assert_eq!(resolve(&root, "/../etc/passwd"), Target::Missing);
        assert_eq!(resolve(&root, "/docs/../about"), Target::Missing);
    }

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
