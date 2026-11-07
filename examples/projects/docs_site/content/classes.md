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
- `extends` and `super`, `override` is checked
- `instanceof` narrows the type
