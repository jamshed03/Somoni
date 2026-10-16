# Somoni

A small, strongly typed programming language, interpreted in Rust.

## Run

    cargo run -- examples/hello.smn

## Tests

    cargo test

Die Tests sind `.smn`-Programme mit erwarteter Ausgabe in `tests/cases/` (`name.smn` + `name.out`).
Neue Features brauchen neue Tests, geänderte Ausgaben brauchen angepasste `.out`-Dateien.
Regeln: `docs/Architektur.md`, Abschnitt 4.

## Status

v0.1.0: int, float, bool, string, var/const, assignment, print, type checker.
