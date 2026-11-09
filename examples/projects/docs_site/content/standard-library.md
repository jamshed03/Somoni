# Standard library

Built-in functions like `print` and `len`, plus methods on `string`, `float`, `list` and `map`. Methods never
change their value, they return a new one.

```smn
var name = "  Ali  ";
print(name.trim().upper());
print("ab,cd,ef".split(","));

var numbers = [5, 2, 9, 1];
print(numbers.sort());
print(numbers.filter(fn(n) => n > 2).map(fn(n) => n * 10));
print(numbers.reduce(0, fn(sum, n) => sum + n));

print(min(3, 7));
print((1.0 / 3.0).format(2));
```

- Strings: `trim`, `upper`, `lower`, `split`, `contains`, `starts_with`, `ends_with`, `replace`, `slice`, `index_of`, `pad_left`, `pad_right`, `repeat`, `escape_html`
- Lists: `contains`, `index_of`, `reverse`, `slice`, `sort`, `sort_desc`, `remove`, `join`, and with a lambda: `map`, `filter`, `reduce`, `any`, `all`, `sort_by`, `sort_by_desc`
- Maps: `values()`, `remove`, `get(k, default)`, plus the functions `len`, `has(m, k)`, `keys(m)`
- Input and output: `print`, `write` (without a line break), `input()` (one line, `null` at the end), `read_file`, `write_file`, `append_file` and `copy_file` (with a `result`; `copy_file` copies bytes, so it also works for images), `capture(fn() => ...)` (what a function prints, as a string)
- Command line: `args()` gives the arguments after the file as a `list<string>`, so `somoni greet.smn Ali 30` gives `["Ali", "30"]`
- Time: `now()` gives the milliseconds since 1970 as an `int`, like `Date.now()` in JavaScript; `now() - start` measures how long something took
- Chance: `random(1, 6)` gives an `int` from 1 to 6, both included; good for games and examples, not for passwords
- Math functions: `abs`, `min`, `max`, `sqrt`, `pow`, `floor`, `ceil`, `round`
- Not found is `null`, not `-1`: `"abc".index_of("z")` has type `int?`
- `somoni --explain T057` and the other codes explain every rule in detail, with a working and a failing example
