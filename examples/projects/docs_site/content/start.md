# What is Somoni?

Somoni is a strongly typed language with curly braces. A type checker finds errors before the program runs, a VM runs the bytecode.

```smn
print("Hello Somoni");

var name = "Ali";
print("Hello {name}!");
```

- Run a program: `somoni file.smn`
- Named after Ismoil Somoni
- Interpreter written in Rust: lexer, parser, type checker, compiler, VM
