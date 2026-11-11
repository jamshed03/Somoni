# Examples

Programs written in Somoni. Run one from the root of the repository:

```
cargo run --release -- examples/hello.smn
```

Every program has a `.out` with its expected output, and `cargo test` runs all of them, so they always work with the
current Somoni. A `.in` next to a program is typed in as input, a `.args` gives its arguments (see `CONTRIBUTING.md`).

## `hello.smn`

The first program: a constant, a string with a value in it, and `print`.

## `algorithms/`

Classic algorithms, one small program each: ciphers, conversions, data structures, graphs, maths with Project Euler,
searches, sorting and strings. `algorithms/README.md` lists the folders.

## `projects/`

Larger programs with several files, each showing a different side of Somoni.

| Project          | What it is                                                     | What it shows                                                                  |
| ---------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `guessing_game/` | Guess a number from 1 to 100, with a hint after every try      | `input()`, `while (var ...)`, `random`, `args()`, `int(...)` with a `result`   |
| `web/`           | A small site: a template page, a normal program, a search page | `somoni serve examples/projects/web`, templates, `query("q")`                  |
| `blocks/`        | A falling blocks game for the terminal, later the browser      | hexagonal architecture, interfaces, factory, observer, dependency injection    |
| `docs_site/`     | The documentation of Somoni, built with Somoni                 | modules, templates with `@props` and `@foreach`, Markdown in Somoni, files     |

`docs_site/` writes one HTML file per page into `docs_site/site/`:

```
cargo run --release -- examples/projects/docs_site/main.smn
```
