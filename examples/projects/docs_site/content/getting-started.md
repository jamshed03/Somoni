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
- `somoni file.smn` runs the file
- `somoni --bytecode file.smn` shows its bytecode instead of running it
- `somoni --explain T003` explains an error code, with an example
- `somoni serve folder` serves a folder of programs and files on `http://localhost:8000` (`--port` to change it)
