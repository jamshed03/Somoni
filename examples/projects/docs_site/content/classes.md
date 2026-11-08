# Classes and interfaces

Classes with a constructor, visibility, inheritance and interfaces.

```smn
interface Shape {
    area(): float;
}

class Circle implements Shape {
    private r: float;

    constructor(r: float) {
        this.r = r;
    }

    area(): float {
        return 3.14 * this.r * this.r;
    }
}
```

- `public`, `private`, `protected`, `readonly`, `static`, `abstract`
- `static` methods and fields belong to the class: `User::guest()`, `User::count`, inherited by subclasses
- Objects are references (`==` asks "the same object?"), structs stay copies
- `extends` and `super`, `override` is checked
- `instanceof` narrows the type
