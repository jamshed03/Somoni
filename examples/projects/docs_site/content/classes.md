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
- `instanceof` narrows the type; with an interface the variable keeps its class too, so `p: Pet` becomes `Pet & Named`:

```smn
interface Named {
    label(): string;
}

class Pet {
    name: string;

    constructor(name: string) {
        this.name = name;
    }
}

class Cat extends Pet implements Named {
    constructor() {
        super("Mia");
    }

    label(): string {
        return "cat";
    }
}

fn show(p: Pet): void {
    if (p instanceof Named) {
        print("{p.name} is a {p.label()}");
    }
}

show(new Cat());
```
