<img src="somoni-logo.svg" alt="Somoni logo" width="96" height="96">

# Somoni

[![Test](https://github.com/jamshed03/Somoni/actions/workflows/test.yml/badge.svg)](https://github.com/jamshed03/Somoni/actions/workflows/test.yml)

A small, strongly typed programming language, interpreted in Rust. Every type is checked before a program runs,
and every error has a code that points to the exact place. Named after Ismoil Somoni (Tajikistan); source files end
in `.smn`.

```
struct User {
    name: string,
    age: int
}

var users = [User { name: "Ali", age: 30 }, User { name: "Bea", age: 17 }];
for (user: User in users.filter(fn(u) => u.age >= 18)) {
    print("Hello {user.name}!");
}
```

A mistake stops the program before anything runs:

```
Type error [T003]: Line 1: cannot assign string to int
  1 | var age: int = "30";
    |                ^^^^
```

## Install

You need [Rust](https://www.rust-lang.org/tools/install). Then:

    cargo install --git https://github.com/jamshed03/Somoni

This puts `somoni` into `~/.cargo/bin`, which the Rust installer adds to your `PATH`. Run it again with `--force`
to update, `cargo uninstall somoni` removes it.

    somoni                      # try Somoni line by line in the REPL (Ctrl+D ends it)
    somoni hello.smn            # run a program
    somoni --explain T003       # explain an error code, with an example
    somoni serve my-site        # serve a folder as web pages on http://localhost:8000

In a clone of this repository, `cargo run -- examples/hello.smn` does the same without installing.

## Documentation

The [documentation](examples/projects/docs_site/content/) starts with
[Getting started](examples/projects/docs_site/content/getting-started.md) and covers types, collections, control
flow, functions, errors as values, classes, modules, the standard library, templates, the web server and best
practices. The site itself is built with Somoni: `cargo run -- examples/projects/docs_site/main.smn`.

## Contributing

`cargo test` runs every test. How tests and examples are checked: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT, see [LICENSE](LICENSE).
