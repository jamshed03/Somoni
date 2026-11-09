# Web server

`somoni serve folder` serves every `.smn` and `.template.smn` file in `folder` as a page: everything the program
prints becomes the response.

```smn
const topics = ["templates", "lambdas", "string interpolation"];

print("<!doctype html>");
print("<html lang=\"en\"><head><meta charset=\"utf-8\"><title>Search</title></head><body>");
print("<form action=\"search\"><input name=\"q\"> <button>Search</button></form>");
if (var q = query("q")) {
    var found = topics.filter(fn(topic) => topic.contains(q.lower()));
    print("<p>Found {len(found)} for '{q.escape_html()}'.</p>");
} else {
    print("<p>Nothing searched yet on {request_path()}.</p>");
}
print("</body></html>");
```

Saved as `search.smn`, this page answers `GET /search?q=lambda`; outside `somoni serve`, `query` and `form` give `null`.

- `query("q")` reads `?q=...` from the address, `form("name")` an `<input name="name">` of a `POST` form
- `request_path()` and `request_method()` read the rest of the request
- `set_status(404)` changes the response code, `redirect("/")` sends the browser elsewhere
- `somoni serve folder --port 8080` uses another port than 8000
- A `name.template.smn` file works the same way: its result is the page

Files under the server:

- `read_file`, `write_file`, `append_file`, `copy_file` and `make_dir` stay inside `folder`: an absolute path, a `..` that leads out of it or a link that points outside gives `err("cannot read '../x': outside the served folder")`
- They never write a file whose path holds `.smn`, so a file name from a form cannot become a page: `write_file("hack.smn", text)` gives `err("cannot write 'hack.smn': somoni serve does not write .smn files")`
- On the command line (`somoni file.smn`) these limits do not apply
