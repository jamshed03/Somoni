#!/bin/sh
# Measures Somoni against PHP, Node.js and Python with hyperfine.
# Every program exists in all four languages with the same algorithm and output.
# Usage: bench/run.sh            all benchmarks
#        bench/run.sh fib loop   only these
set -e
cd "$(dirname "$0")/.."
cargo build --release --quiet

for name in ${*:-fib loop strings reads}; do
    hyperfine --warmup 1 --runs 5 -N \
        -n somoni "./target/release/somoni bench/$name.smn" \
        -n php "php bench/$name.php" \
        -n node "node bench/$name.js" \
        -n python "python3 bench/$name.py"
done
