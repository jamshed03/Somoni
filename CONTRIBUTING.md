# Contributing

A program goes through lexer, parser, type checker, compiler and VM; the
[How it works](examples/projects/docs_site/content/how-it-works.md) page shows the way.
`cargo run -- --bytecode file.smn` prints what the compiler produced instead of running it.

## Tests

    cargo test

Every test is a Somoni program with its expected output in `tests/cases/`:
`name.smn` is the program, `name.out` is stdout followed by stderr.
Programs that should fail start with `err_` and must exit with code 1, all others with 0.
A `name.in` next to `name.smn` is given to the program as typed input, a `name.args` gives one argument per line.
The programs in `examples/` are checked the same way, except files in folders named `lib`, which are only imported.
`tests/repl/name.in` is typed into the REPL (`somoni` without a file) line by line, and what it shows must be
`name.out`; without a terminal the REPL shows no prompt.

-   New features need new tests (one that works, one `err_` case per new error).
-   A new error code needs an explanation in `src/explain/` (`somoni --explain CODE`); `cargo test` runs its
    examples: a `Wrong:` example must give the code, a `Right:` example must run without errors.
-   Every ```` ```smn ```` and ```` ```smn-bad ```` block in the pages of the documentation site
    (`examples/projects/docs_site/content/*.md`) must run without errors, every ```` ```smn-wrong T006 ```` block must
    give exactly that error; `cargo test` runs them too.
-   If a change modifies an output on purpose, update the affected `.out` files and check them by hand.
-   `cargo fmt` and `cargo clippy` should have nothing to say.
