//! `somoni serve`: a small web server that answers every request with a file of one folder.
//! A Somoni program runs and what it writes is the page; any other file is sent as it is.

use std::fs;
use std::io::{self, BufReader};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use crate::vm::{self, Request};
use crate::{build, render};

use http::{Incoming, Response, content_type, page, read_request, write_response};
use url::{decode, parse_query, split_target};

mod http;
mod url;

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
            };
            run_page(&path, data)
        }
        Target::File(_) if method == "POST" => page(405, "A file cannot take a POST."),
        Target::File(path) => match fs::read(&path) {
            Ok(body) => Response {
                status: 200,
                content_type: content_type(&path),
                location: None,
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
        let ran = vm::run_captured(&program, request);
        ran.ended
            .map(|()| (ran.status, ran.location, ran.output))
            .map_err(|error| render(&error, &files))
    });
    match output {
        Ok((status, location, html)) => Response {
            status,
            content_type: "text/html; charset=utf-8",
            location,
            body: html.into_bytes(),
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
}
