# Getting started

Somoni is one command run from the terminal; there is nothing to configure.

Install it from the source (needs Rust, see rustup.rs):

```text
cargo install --git https://github.com/jamshed03/Somoni
```

Save a file `hello.smn`:

```smn
print("Hello, Somoni!");
```

Then run it:

```text
somoni hello.smn
```

- The install command with `--force` updates Somoni, `cargo uninstall somoni` removes it
- `somoni` without a file starts the REPL (below)
- `somoni file.smn` runs the file
- `somoni --bytecode file.smn` shows its bytecode instead of running it
- `somoni --explain T003` explains an error code, with an example
- `somoni serve folder` serves a folder of programs and files on `http://localhost:8000` (`--port` to change it)

## Try it line by line

`somoni` without a file starts the REPL: type Somoni and see the result right away, with its type. Earlier lines
stay known, an input goes on over several lines while a bracket is open, `import` reads files from the current
folder, and Ctrl+D ends it.

```text
> var names = ["Ali", "Bea"];
> names.map(fn(n) => n.upper())
["ALI", "BEA"]: list<string>
> fn twice(n: int): int {
.     return n * 2;
. }
> twice(len(names))
4: int
> twice("x")
Type error [T014]: Line 1: cannot use string as argument of 'twice'
  1 | twice("x")
    |       ^^^
```
