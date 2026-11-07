# Modules

A file marks what it shares with `export`. Other files get it with `import`.

```text
import { square } from "math.smn";
import * as text from "text.smn";

print(square(4));
```

- `export` before `fn`, `struct`, `enum`, `class`, `interface` or `const`
- Every file is read once, import cycles are reported
