# Error codes

Every error has a code, like `T006` or `R005`: a letter for the kind, three digits so it can be looked up.

```smn
var list = [1, 2, 3];
print(list[len(list) - 1]);
```

The line before works. `list[3]` instead would fail with a code:

```text
Runtime error [R005]: Line 2: index 3 is out of range (list has 3 elements)
  2 | print(list[3]);
    |       ^^^^^^^
```

`somoni --explain R005` explains it in full, with a `Wrong:` and a `Right:` example.

- `L` lexer (a character makes no sense), `P` parser (the grammar does not fit), `M` modules (an import fails),
  `T` type (the type checker rejects the program before it runs), `R` runtime (the program runs but fails, like
  division by zero or a bad index)
- `cargo test` runs every explanation's `Wrong:` and `Right:` examples, so none of them ever goes stale
