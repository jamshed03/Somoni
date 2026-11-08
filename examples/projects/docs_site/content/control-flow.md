# Control flow

Blocks `{ }` have their own scope: a name declared inside only exists until the closing `}`.

```smn
var x = 5;
{
    var y = x * 2;
    print(y > 5);
}
print(x);
```

`if`, `else if` and `else` need a `bool` condition and braces.

```smn
var n = 3;
if (n > 0) {
    print("positive");
} else if (n < 0) {
    print("negative");
} else {
    print("zero");
}
```

`while` and `for` loop with `break` and `continue`.

```smn
for (var i = 0; i < 5; i++) {
    if (i == 2) {
        continue;
    }
    if (i == 4) {
        break;
    }
    print(i);
}
```

`for`-each runs over a list, a map or a string.

```smn
for (name: string in ["Ali", "Bea"]) {
    print(name);
}
```

`match` chooses a branch by an enum variant, a result, or a value; it must cover every case, or end with `_`.

```smn
enum Color {
    Red,
    Green,
    Blue
}

var c = Color.Green;
match (c) {
    Color.Red => {
        print("red");
    }
    Color.Green => {
        print("green");
    }
    _ => {
        print("blue");
    }
}
```

After `if (a != null)`, the value of `a` can be used in the block without `?` ("narrowing"). `if (var name = value)` checks a value that can be null, without a variable declared before it.

```smn
fn first_positive(numbers: list<int>): int? {
    for (n: int in numbers) {
        if (n > 0) {
            return n;
        }
    }
    return null;
}

if (var found = first_positive([-2, -1, 3, 4])) {
    print(found);
} else {
    print("none");
}
```

- `break` and `continue` only work inside a loop
- `for`-each over a map gives the key and the value: `for (key: string, value: int in m) { ... }`
