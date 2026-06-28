#!/usr/bin/env bash
set -euo pipefail
cargo clippy --workspace --fix
cargo fmt --all
git commit -a -m "fmt"