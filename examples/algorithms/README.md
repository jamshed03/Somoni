# Algorithmen in Somoni

```
cargo run --release -- examples/algorithms/sorting/quick_sort.smn
```

Jedes Beispiel hat eine `.out` mit seiner erwarteten Ausgabe, `cargo test` prüft alle (siehe `docs/Architektur.md`, Abschnitt 4).
Ein neues Beispiel braucht deshalb auch eine `.out`. In `lib/` liegen nur Module, die die Beispiele importieren.

## Ordner

| Ordner                 | Inhalt                                                                                                       |
| ---------------------- | ------------------------------------------------------------------------------------------------------------ |
| `ciphers/`             | Atbash, Caesar, Mono-Alphabetic, Morse, Rail Fence, Vigenère                                                 |
| `conversions/`         | Binär, Hex, Oktal, Geschwindigkeit, Temperatur                                                               |
| `data_structures/`     | Stack, Queue, verkettete Listen, BST, AVL, Splay, Trie, Union-Find, Segment Tree                             |
| `graphs/`              | BFS, DFS, Dijkstra, Bellman-Ford                                                                             |
| `maths/`               | Primzahlen, Sieb, GCD, Fibonacci, Mean/Median/Mode, ...                                                      |
| `maths/project_euler/` | Problem 1 bis 11                                                                                             |
| `neural_networks/`     | Perceptron                                                                                                   |
| `searches/`            | Binary, Exponential, Fibonacci, Interpolation, Jump, Ternary, ...                                            |
| `sorting/`             | Bubble, Count, Gnome, Heap, Insertion, Merge, Quick, Radix, Selection, Shell, Array Keys                     |
| `strings/`             | Anagramm, Palindrom, Vokale, Levenshtein, ...                                                                |
| `lib/`                 | Hilfsfunktionen, die Somoni (noch) nicht eingebaut hat: `chars`, `split`, `lower`, `isqrt`, `exp`, `ln`, ... |
