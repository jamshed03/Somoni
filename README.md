# Somoni

A small, strongly typed programming language, interpreted in Rust.
Named after Ismoil Somoni (Tajikistan). Source files end in `.smn`.

```
const name: string = "Ali";
var x: int = 5;
{
    var y: int = x * 2;   // y only exists inside this block
    print(y > 5 && name == "Ali");
}
print(x % 3);
```

## Run

    cargo run -- examples/hello.smn

## Tests

    cargo test

Every test is a Somoni program with its expected output in `tests/cases/`:
`name.smn` is the program, `name.out` is stdout followed by stderr.
Programs that should fail start with `err_`.

-   New features need new tests (one that works, one `err_` case per new error).
-   If a change modifies an output on purpose, update the affected `.out` files and check them by hand.

## How it works

Pipeline: source text, lexer (tokens), parser (AST), type checker, interpreter.
Type errors are reported with a line number before anything runs.

## Status

Version 0.1.0, in progress:

-   Types `int`, `float`, `bool`, `string`; `var` and `const`; assignment; `print`
-   Operators `+ - * / %`, `== != < > <= >=`, `&& || !` (short-circuit), unary `-`
-   Integer overflow and division by zero are runtime errors
-   Blocks `{ }` with their own scope (inner blocks may shadow outer names)
-   `if`, `else if`, `else` (condition must be `bool`, braces required)
-   Comments `//` and `/* */`

Next: loops, functions.
