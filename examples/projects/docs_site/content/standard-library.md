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
- Math functions: `abs`, `min`, `max`, `sqrt`, `pow`, `floor`, `ceil`, `round`
- Not found is `null`, not `-1`: `"abc".index_of("z")` has type `int?`
- `somoni --explain T057` and the other codes explain every rule in detail, with a working and a failing example
