# Errors as values

A `result<T, E>` is `ok(value)` or `err(reason)`. No exceptions: a function that can fail says so in its type.

```smn
fn divide(a: int, b: int): result<int, string> {
    if (b == 0) {
        return err("division by zero");
    }
    return ok(a / b);
}

fn describe(a: int, b: int): result<string, string> {
    var value = divide(a, b)?;
    return ok("{a} / {b} = {value}");
}

match (divide(6, 0)) {
    ok(value: int) => { print(value); }
    err(reason: string) => { print(reason); }
}
match (describe(6, 2)) {
    ok(text: string) => { print(text); }
    err(reason: string) => { print(reason); }
}
```

- `match` must handle both `ok` and `err`
- `value?` only works inside a function that itself returns a `result`: on `err`, it returns that `err` right away, otherwise it unwraps the value
- `is_ok(r)` and `is_err(r)` check a result without unwrapping it
- `read_file`, `write_file` and `int("abc")` already give a `result`, so the same pattern works for them
