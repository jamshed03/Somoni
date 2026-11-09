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
for (name in ["Ali", "Bea"]) {
    print(name);
}
for (i, name: string in ["Ali", "Bea"]) {
    print("{i}: {name}");
}
```

The type of a loop variable may be left out: it is the type of the elements, so `name` is a `string` and stays one.
With two variables, the first one is the index of a list or string, or the key of a map.

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
    Color.Blue => {
        print("blue");
    }
}

var letter = "e";
match (letter) {
    "a" | "e" | "i" | "o" | "u" => {
        print("vowel");
    }
    _ => {
        print("consonant");
    }
}
```

Over ints and strings, `_` is required; `|` puts several values into one branch.

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

`while (var name = value)` runs as long as the value is not null; the value is computed again before every round.
`while (var line = input())` reads lines until the input ends.

```smn
var text = "a,b,c";
while (var comma = text.index_of(",")) {
    print(text.slice(0, comma));
    text = text.slice(comma + 1, len(text));
}
print(text);
```

- `break` and `continue` only work inside a loop
- `for`-each over a map gives the key and the value: `for (key: string, value: int in m) { ... }`
