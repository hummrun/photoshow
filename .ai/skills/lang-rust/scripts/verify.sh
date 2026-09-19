#!/bin/sh
set -eu
echo "=== Verifying Rust Safe-by-Default & Non-Lexical Lifetimes ==="
prumo tool check-escape-hatches .
exit 0
