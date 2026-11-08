# Collections

Lists, maps and tuples group several values. A `struct` groups named fields with their own type.

```smn
var names = ["Ali", "Bea", "Cem"];
var ages = {"Ali": 30, "Bea": 25};
var point = (3, 4);

struct User {
    name: string,
    age: int
}
var user = User { name: "Ali", age: 30 };

print(names[0]);
print(ages["Bea"]);
print(point.0 + point.1);
print(user.name);
```

- `list<T>`, `map<K, V>` and `(int, string)` carry their element types; `l + [4]` joins lists, `m["Tom"] = 40;` adds or changes an entry
- A value from `l[i]` or `m[k]` is a copy: changing it never reaches the list or map it came from
- `enum Color { Red, Green }` is a fixed set of names, read with `match`, which must cover every variant
- Parts change in place: `l[i] = x;`, `u.name = x;`, also nested like `lines[0].from.x = 3;`; a `const` protects its parts too
- A tuple splits apart with `var (q: int, r: int) = divide(17, 5);`
