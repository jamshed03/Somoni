# Functions and lambdas

Functions have types for their parameters and their result. Lambdas may leave them out where they are known.

```smn
struct User {
    name: string,
    age: int
}

fn square(x: int): int {
    return x * x;
}

var users = [User { name: "Ali", age: 30 }];
print(users.map(fn(u) => u.name));
print(users.filter(fn(u) => u.age > 18));
```

- `map`, `filter`, `reduce`, `any`, `all` and `sort_by` give the lambda its types
- Lambdas copy the outside variables they use
