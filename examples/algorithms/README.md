# Algorithms in Somoni

```
cargo run --release -- examples/algorithms/sorting/quick_sort.smn
```

Every example has a `.out` with its expected output, and `cargo test` checks all of them.
A new example therefore needs a `.out` too. `lib/` only holds modules that the examples import.

## Folders

| Folder                 | Content                                                                                       |
| ---------------------- | --------------------------------------------------------------------------------------------- |
| `ciphers/`             | Atbash, Caesar, mono-alphabetic, Morse, rail fence, Vigenère                                  |
| `conversions/`         | Binary, hex, octal, speed, temperature                                                        |
| `data_structures/`     | Stack, queue, linked lists, BST, AVL, splay, trie, union-find, segment tree                   |
| `graphs/`              | BFS, DFS, Dijkstra, Bellman-Ford                                                              |
| `maths/`               | Primes, sieve, GCD, Fibonacci, mean/median/mode, ...                                          |
| `maths/project_euler/` | Problems 1 to 11                                                                              |
| `neural_networks/`     | Perceptron                                                                                    |
| `searches/`            | Binary, exponential, Fibonacci, interpolation, jump, ternary, ...                             |
| `sorting/`             | Bubble, count, gnome, heap, insertion, merge, quick, radix, selection, shell, array keys      |
| `strings/`             | Anagram, palindrome, vowels, Levenshtein, ...                                                 |
| `lib/`                 | Helpers that Somoni does not have built in (yet): `chars`, `split`, `lower`, `isqrt`, `exp`, `ln`, ... |
