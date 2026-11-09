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

## Operators

```smn
var a = 7;
var b = 2;
print(a / b);
print(a % b);
print(a > b && b > 0);
a += 1;
a++;
print(a);
```

- `int / int` truncates; use a `float` for a precise result
- Division by zero and a result too large for an `int` or a `float` stop the program with an error: Somoni has no `inf` and no `NaN`
- `&&` and `||` short-circuit: the right side only runs if it is needed
- `+= -= *= /= %=`, and `i++`/`i--` as statements, not expressions

## Strings

```smn
var name = "Ali";
print("Hello {name}!");
print("Line one\nLine two");

var code = """
    fn square(x: int): int {
        return x * x;
    }
    """;
print(code);
```

- `"{value}"` puts a value into a string ("interpolation"); only `int`, `float`, `bool` and `string` fit
- Escapes: `\n`, `\r`, `\t`, `\"`, `\\`, `\{`, `\}`
- A raw string `"""..."""` has no escapes and no interpolation; the indentation of its closing `"""` is removed from every line
