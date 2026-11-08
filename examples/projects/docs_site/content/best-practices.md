# Best practices

Habits that keep Somoni code short and safe. Every Wrong example is checked: either Somoni stops it with the error
shown, or it runs but is harder to read or easier to break than the Right one below it.

## Put values into text with interpolation

Somoni never turns a number into text by itself, so `+` between a string and an int is an error.

```smn-wrong T006
var age = 30;
print("Age: " + age);
```

```smn
var age = 30;
print("Age: {age}");
```

## Use null for "not found", not -1

A `-1` looks like a real number and is easy to use by mistake. An `int?` must be checked before it is used.

```smn-bad
var names = ["Ali", "Bea"];
var found = -1;
for (var i = 0; i < len(names); i++) {
    if (names[i] == "Bea") {
        found = i;
    }
}
print(found);
```

```smn
var names = ["Ali", "Bea"];
if (var found = names.index_of("Bea")) {
    print(found);
}
```

## Check an optional value before you use it

```smn-wrong T006
var names = ["Ali", "Bea"];
var found: int? = names.index_of("Bea");
print(found + 1);
```

```smn
var names = ["Ali", "Bea"];
var found: int? = names.index_of("Bea");
if (found != null) {
    print(found + 1);
}
```

## Unwrap a result before you use it

`int("5")` gives a `result`, because a text may not be a number. Handle both cases with `match`.

```smn-wrong T006
var n = int("5");
print(n + 1);
```

```smn
match (int("5")) {
    ok(n: int) => { print(n + 1); }
    err(reason: string) => { print(reason); }
}
```

## Use const for values that never change

The type checker then stops every accidental change.

```smn-wrong T005
const limit = 10;
limit = 20;
```

```smn
const limit = 10;
var count = 0;
count = limit + 1;
print(count);
```

## Name every enum variant in match

With `_`, a new variant silently takes the default branch. Without it, the type checker lists every `match` that
must handle it.

```smn-bad
enum Status { Draft, Published, Archived }

fn label(s: Status): string {
    match (s) {
        Status.Draft => { return "draft"; }
        _ => { return "live"; }
    }
}
print(label(Status.Archived));
```

```smn
enum Status { Draft, Published, Archived }

fn label(s: Status): string {
    match (s) {
        Status.Draft => { return "draft"; }
        Status.Published => { return "live"; }
        Status.Archived => { return "archived"; }
    }
}
print(label(Status.Archived));
```

## Say what you want with list methods

`filter`, `map` and `any` say what a loop is for in one line.

```smn-bad
var numbers = [3, 8, 1, 9];
var big: list<int> = [];
for (n: int in numbers) {
    if (n > 4) {
        big += [n];
    }
}
print(big);
```

```smn
var numbers = [3, 8, 1, 9];
print(numbers.filter(fn(n) => n > 4));
```

## Escape text before it goes into HTML

Text from a user may contain `<script>`. In a template `{{ }}` escapes it; in a program use `escape_html()`.

```smn-bad
var comment = "<script>alert(1)</script>";
print("<p>{comment}</p>");
```

```smn
var comment = "<script>alert(1)</script>";
print("<p>{comment.escape_html()}</p>");
```
