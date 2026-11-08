# How it works

A Somoni program goes through five steps before anything runs: the lexer turns text into tokens, the parser
turns tokens into a tree, the type checker checks that tree, the compiler turns it into bytecode, and the VM
runs that bytecode.

```text
var x: int = 1 + true;
```

Nothing in this program ever runs: the type checker rejects it before the compiler or the VM see it.

```text
Type error [T006]: Line 1: cannot use operator on int and bool
  1 | var x: int = 1 + true;
    |                ^
```

- Type errors are always reported before a program starts; a runtime error like division by zero only happens while it runs
- Every error has a code (like `T006`) and a kind: `L` lexer, `P` parser, `M` modules, `T` type, `R` runtime
- `cargo run -- --bytecode file.smn` prints the bytecode the compiler produced, instead of running it
- `cargo run -- --explain T006` explains a code in full, with a working and a failing example
