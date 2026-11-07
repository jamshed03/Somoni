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

    cargo run -- examples/hello.smn              # run it
    cargo run -- --bytecode examples/hello.smn   # show the bytecode instead of running it
    cargo run -- --explain T003                  # explain an error code, with an example
    cargo run -- examples/projects/docs_site/main.smn   # build the documentation site into examples/projects/docs_site/site/
    cargo run -- serve examples/projects/web            # serve a folder on http://localhost:8000 (--port to change it)

## Tests

    cargo test

Every test is a Somoni program with its expected output in `tests/cases/`:
`name.smn` is the program, `name.out` is stdout followed by stderr.
Programs that should fail start with `err_` and must exit with code 1, all others with 0.

-   New features need new tests (one that works, one `err_` case per new error).
-   A new error code needs an explanation in `src/explain/` (`somoni --explain CODE`); `cargo test` runs its
    examples: a `Wrong:` example must give the code, a `Right:` example must run without errors.
-   If a change modifies an output on purpose, update the affected `.out` files and check them by hand.

## How it works

Pipeline: source text, lexer (tokens), parser (AST), type checker, compiler (bytecode), VM.
Type errors are reported before anything runs. Every error has a code and points to the exact place:

```
Type error [T006]: Line 1: cannot use operator on int and bool
  1 | var x: int = 1 + true;
    |                ^
```

## Status

Version 0.9.0: the core of the language plus lists, structs, optional types, maps, enums, results, tuples, classes, modules, function values, lambdas, type inference, a standard library, files, string interpolation, templates and a small web server, compiled to bytecode and run in a VM:

-   Types `int`, `float`, `bool`, `string` (with `\n`, `\r`, `\t`, `\"`, `\\`, `\{`, `\}` and interpolation `"Hello {name}"`, raw strings `"""..."""` without escapes); `var` and `const`, with or without a type (`var count = 0;` is an `int`); assignment; `print`; a top-level `const` is visible in functions and methods
-   Operators `+ - * / %`, `== != < > <= >=` (also for strings), `&& || !` (short-circuit), unary `-`; `+= -= *= /= %=`, `i++` and `i--` as statements
-   Integer overflow and division by zero are runtime errors
-   Blocks `{ }` with their own scope (inner blocks may shadow outer names)
-   `if`, `else if`, `else` (condition must be `bool`, braces required)
-   `while`, `for` and `for (x: int in list)` over lists, maps and strings, with `break` and `continue`
-   Lists `list<int>` with `[1, 2, 3]`, `l[i]`, `len(l)`, joined with `l + [4]` (values are copied, a change never reaches another variable)
-   Structs `struct User { name: string, age: int }` with `User { name: "Ali", age: 30 }` and `u.name` (copied like lists)
-   Optional types `int?` with `null`; an `int` fits where an `int?` is expected
-   Enums `enum Color { Red, Green }` with `match` that must cover every variant (or use `_`); `match` also over ints and strings (`"+" => { ... }`, `_` required), several values in one branch (`"a" | "e" => { ... }`)
-   Errors as values: `result<int, string>` with `ok(x)` and `err(e)`, read with `match` (`ok(n: int) => { ... }`), passed on with `value?` inside functions that return a result, tested with `is_ok(r)` and `is_err(r)`
-   Tuples `(int, string)` with `(1, "one")` and `p.0`, also to return several values from a function, taken apart with `var (q: int, r: int) = teilen(17, 5);`
-   Maps `map<string, int>` with `{"Ali": 30}`, `m["Ali"]`, `m["Tom"] = 40;`, `len`, `has(m, k)`, `keys(m)` (insertion order)
-   Changing parts: `l[i] = x;`, `u.name = x;`, also nested like `lines[0].from.x = 3;` (copy-on-write, `const` protects the parts too)
-   String methods `s.trim()`, `upper`, `lower`, `split`, `contains`, `starts_with`, `ends_with`, `replace`, `slice`, `index_of`, `pad_left`, `pad_right`, `repeat`, `escape_html`, and `x.format(2)` on floats
-   List methods `l.contains(x)`, `index_of`, `reverse`, `slice`, `sort`, `sort_desc`, `remove`, `join`, map methods `m.values()`, `remove`, `get(k, default)` (all return a new value, see `docs/Bibliothek.md`)
-   With functions: `l.map(f)`, `filter`, `reduce(start, f)`, `any`, `all`, `sort_by(key)`, `sort_by_desc(key)`, e.g. `users.sort_by(fn(u) => u.alter)` (the lambda gets its types from the method)
-   Math functions `abs`, `min`, `max`, `sqrt`, `pow`, `floor`, `ceil`, `round`
-   Built-in functions `print`, `write` (without a line break), `capture(fn() => ...)` (gives back what the function writes as a string), `input` (one line, `null` at the end), `read_file`/`write_file` (with a `result`, paths start at the folder of the file), `query("q")`, `form("name")`, `request_path()`, `request_method()`, `set_status(404)` and `redirect("/")` for pages under `somoni serve`, `int`, `float`, `string`, `len`, `has`, `keys`, `ok`, `err`, `is_ok`, `is_err` (conversions are always explicit; `int("5")` gives a `result` because a text may not be a number)
-   Templates `page.template.smn`: HTML text with `{{ value }}` (escaped), `{!! html !!}` (raw), `{{-- comments --}}`, `@if`/`@elseif`/`@else`/`@endif` and `@foreach`/`@endforeach`; run with `somoni page.template.smn > page.html`. With `@import` and `@props(page: Page)` a template is a function that other files import and call (`card(page)`), also from another template with `@include(card(page))`
-   Own functions with `fn name(a: int): int { ... }`, `return` and recursion; functions see only their parameters, every path must return a value
-   Functions as values with types like `fn(int): int`: `var f: fn(int): int = square; f(3)`, also in lists and fields (`button.action(x)`)
-   Lambdas `fn(x: int): int => x * 2` or with a block, also without types where they are known (`var twice: fn(int): int = fn(x) => x * 2;`); they copy the outside variables they use (and `this`) when they are made
-   Comments `//` and `/* */`
-   Classes: fields with start values, `constructor(...)` with `this`, methods (`u.greet()`), `public`/`private`, `readonly` fields, `static` methods and fields (`User::gast()`, `User::anzahl`, inherited by subclasses), `abstract` classes and methods, `instanceof` with narrowing, private and protected constructors, `interface` with `implements`, `extends` with `super`, `override` and `protected` (methods chosen at runtime), `new User("Ali", 30)`; objects are references (`==` is identity), structs stay copies
-   Modules: `import { square, Point as P } from "math.smn";` or `import * as math from "math.smn";` with `math.square(3)`, and `export fn`/`struct`/`enum`/`class`/`interface`/`const` (imported files contain only declarations, their constants only fixed values)

-   Null checks: after `if (a != null)` the value of `a` can be used; `if (var avg: float = s.average()) { ... }` checks a value without an extra variable

