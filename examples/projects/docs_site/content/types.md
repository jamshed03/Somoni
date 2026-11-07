# Variables and types

Every variable has a fixed type. It may be left out, then it is the type of the value.

```smn
var count = 0;              // int
const pi: float = 3.14;
var names = ["Ali", "Bea"];    // list<string>
var ages = {"Ali": 30};       // map<string, int>
var found: int? = null;       // int or null

if (found != null) {
    print(found + 1);
}
```

- int, float, bool, string, list, map, tuple, struct, enum, result
- No silent conversion: `"5" + 1` is an error
- A value with `?` must be checked for null before it is used
