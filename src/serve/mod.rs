//! `somoni serve`: a small web server that answers every request with a file of one folder.
//! A Somoni program runs and what it writes is the page; any other file is sent as it is.

use std::fs;
use std::io::{self, BufReader, ErrorKind};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::vm::{self, Request};
use crate::{build, render};

use http::{Incoming, Response, content_type, page, read_request, write_response};
use url::{decode, parse_query, split_target};

mod http;
mod url;

/// How long a connection may take to send its request before the server closes it.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

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
    listen(listener, &root);
    Ok(())
}

/// Answers the connections that come in on `listener` with the files of `root`, each in a thread of its own, so a
/// connection that sends nothing (a browser opens such ones ahead) does not keep the others waiting.
fn listen(listener: TcpListener, root: &Path) {
    for stream in listener.incoming() {
        let root = root.to_path_buf();
        let answered = stream.map(|stream| thread::spawn(move || answer_logged(stream, &root)));
        if let Err(err) = answered {
            eprintln!("connection failed: {}", err);
        }
    }
}

/// Answers one connection and logs a failure; a connection that sent nothing in time is closed quietly.
fn answer_logged(stream: TcpStream, root: &Path) {
    match answer(stream, root) {
        Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
        Err(err) => eprintln!("request failed: {}", err),
        Ok(()) => {}
    }
}

/// Reads one request from `stream`, writes the answer and logs it, like `GET /about 200`.
fn answer(mut stream: TcpStream, root: &Path) -> io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    let (line, response) = match read_request(&mut BufReader::new(&stream))? {
        Some(request) => (request.line.clone(), respond(root, &request)),
        None => (
            "(unreadable request)".to_string(),
            page(400, "The request cannot be read."),
        ),
    };
    println!("{} {}", line, response.status);
    write_response(&mut stream, &response)
}

/// Returns the answer for a GET or POST `request`: the output of a program, a file, or a page for an error.
fn respond(root: &Path, request: &Incoming) -> Response {
    let method = request.method.as_str();
    if method != "GET" && method != "POST" {
        return page(405, "Only GET and POST requests are answered.");
    }
    let (raw_path, raw_query) = split_target(&request.target);
    let parts = (
        decode(raw_path, false),
        parse_query(raw_query),
        form_fields(request),
    );
    let (Some(url), Some(query), Some(form)) = parts else {
        return page(
            400,
            &format!("The request for {} cannot be read.", request.target),
        );
    };
    match resolve(root, &url) {
        Target::Program(path) => {
            let method = method.to_string();
            let data = Request {
                method,
                path: url,
                query,
                form,
                root: root.to_path_buf(),
            };
            run_page(&path, data)
        }
        Target::File(_) if method == "POST" => page(405, "A file cannot take a POST."),
        Target::File(path) => match fs::read(&path) {
            Ok(body) => Response {
                status: 200,
                content_type: content_type(&path),
                location: None,
                headers: Vec::new(),
                body,
            },
            Err(err) => page(500, &format!("Cannot read {}: {}", url, err)),
        },
        Target::Missing => page(404, &format!("There is nothing at {}.", url)),
    }
}

/// Returns the fields of a form sent with POST, none for other requests and bodies; None if they cannot be read.
fn form_fields(request: &Incoming) -> Option<Vec<(String, String)>> {
    let is_form = request
        .content_type
        .starts_with("application/x-www-form-urlencoded");
    if request.method != "POST" || !is_form {
        return Some(Vec::new());
    }
    parse_query(std::str::from_utf8(&request.body).ok()?)
}

/// Finds what the decoded `path` stands for in `root`: `/` and `/docs/` look for `index`, `/about` for `about.smn`
/// or `about.template.smn` or a file `about`; `.smn` files are never sent, and no path leaves `root`.
fn resolve(root: &Path, path: &str) -> Target {
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

/// Runs the program `path` for `request` and returns what it writes as the page, or a page 500 with its error.
fn run_page(path: &Path, request: Request) -> Response {
    let built = build(&path.to_string_lossy());
    let output = built.and_then(|(program, files)| {
        let mut ran = vm::run_captured(&program, request);
        let ended = std::mem::replace(&mut ran.ended, Ok(()));
        ended.map(|()| ran).map_err(|error| render(&error, &files))
    });
    match output {
        Ok(ran) => Response {
            status: ran.status,
            content_type: "text/html; charset=utf-8",
            location: ran.location,
            headers: ran.headers,
            body: ran.output.into_bytes(),
        },
        Err(message) => page(500, &message),
    }
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
            resolve(&root, "/about"),
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

    /// Writes the program `source` as the file `name` into the served folder `root`.
    fn page_file(root: &Path, name: &str, source: &str) {
        fs::write(root.join(name), source).unwrap();
    }

    /// Sends the raw HTTP request `raw` through `read_request` and `respond`, like the server does without the network.
    fn send(root: &Path, raw: &str) -> Response {
        let incoming = read_request(&mut raw.as_bytes()).unwrap().unwrap();
        respond(root, &incoming)
    }

    /// Sends `GET target` and returns the status and the body as text.
    fn get(root: &Path, target: &str) -> (u16, String) {
        let response = send(root, &format!("GET {} HTTP/1.1\r\nHost: x\r\n\r\n", target));
        (response.status, String::from_utf8(response.body).unwrap())
    }

    #[test]
    fn a_program_answers_with_what_it_prints() {
        let root = site("program");
        page_file(
            &root,
            "hello.smn",
            "if (var name = query(\"name\")) {\n    print(\"hi {name}\");\n}\n",
        );
        assert_eq!(get(&root, "/hello?name=Ann"), (200, "hi Ann\n".to_string()));
    }

    #[test]
    fn a_form_sent_with_post_reaches_the_program() {
        let root = site("post");
        page_file(
            &root,
            "form.smn",
            "if (var name = form(\"name\")) {\n    print(name);\n}\n",
        );
        let body = "name=Ann+B";
        let raw = format!(
            "POST /form HTTP/1.1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let response = send(&root, &raw);
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        assert_eq!(response.body, b"Ann B\n");
    }

    #[test]
    fn a_redirect_and_a_runtime_error_set_the_status() {
        let root = site("status");
        page_file(&root, "away.smn", "redirect(\"/hello\");\n");
        page_file(
            &root,
            "broken.smn",
            "var zero: int = 0;\nprint(5 / zero);\n",
        );
        let away = send(&root, "GET /away HTTP/1.1\r\n\r\n");
        assert_eq!(
            (away.status, away.location.as_deref()),
            (303, Some("/hello"))
        );
        assert_eq!(get(&root, "/broken").0, 500);
    }

    #[test]
    fn a_page_cannot_write_source_files_or_read_outside() {
        let root = site("files");
        fs::write(root.parent().unwrap().join("secret.txt"), "secret").unwrap();
        page_file(
            &root,
            "unsafe.smn",
            "if (var name = query(\"write\")) {\n    print(write_file(name, \"print(1);\"));\n}\nif (var path = query(\"read\")) {\n    print(read_file(path));\n}\n",
        );
        let (_, written) = get(&root, "/unsafe?write=hack.smn");
        assert!(written.contains("does not write .smn files"), "{}", written);
        assert!(!root.join("hack.smn").exists());
        let (_, read) = get(&root, "/unsafe?read=../secret.txt");
        assert!(read.contains("outside the served folder"), "{}", read);
        assert!(!read.contains("secret\""), "{}", read);
    }

    #[test]
    fn a_page_opens_databases_only_inside_its_folder() {
        let root = site("database");
        page_file(
            &root,
            "db.smn",
            "if (var path = query(\"path\")) {\n    print(db_open(path));\n}\n",
        );
        let (_, outside) = get(&root, "/db?path=../outside.db");
        assert!(outside.contains("outside the served folder"), "{}", outside);
        assert!(!root.parent().unwrap().join("outside.db").exists());
        let (_, source) = get(&root, "/db?path=hack.smn");
        assert!(source.contains("does not write .smn files"), "{}", source);
        let (_, inside) = get(&root, "/db?path=docs/app.db");
        assert_eq!(inside, "ok(<database docs/app.db>)\n");
        assert!(root.join("docs/app.db").exists());
    }

    #[test]
    fn a_connection_that_sends_nothing_does_not_block_the_others() {
        use std::io::{Read, Write};
        use std::time::Duration;
        let root = site("idle");
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || listen(listener, &root));
        let _idle = TcpStream::connect(address).unwrap();
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client
            .write_all(b"GET /style.css HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut answer = String::new();
        let read = client.read_to_string(&mut answer);
        assert!(
            read.is_ok() && answer.starts_with("HTTP/1.1 200"),
            "{:?} {:?}",
            read,
            answer
        );
    }

    #[test]
    fn a_page_sets_its_own_headers() {
        let root = site("headers");
        page_file(
            &root,
            "api.smn",
            "set_header(\"Content-Type\", \"application/json\");\nset_header(\"X-Count\", \"1\");\nset_header(\"x-count\", \"2\");\nwrite(\"[]\");\n",
        );
        let response = send(&root, "GET /api HTTP/1.1\r\n\r\n");
        let mut written = Vec::new();
        write_response(&mut written, &response).unwrap();
        let text = String::from_utf8(written).unwrap();
        assert_eq!(text.matches("Content-Type:").count(), 1, "{}", text);
        assert!(
            text.contains("Content-Type: application/json\r\n"),
            "{}",
            text
        );
        assert!(
            text.contains("x-count: 2\r\n") && !text.contains("X-Count: 1"),
            "{}",
            text
        );
        assert!(text.ends_with("\r\n\r\n[]"), "{}", text);
    }
}
