#!/usr/bin/env bash
# Build policies to the WASM components the Aram gate loads.
#   tools/build-policies.sh            build every policy under policy-src/
#   tools/build-policies.sh <name>...  build the named ones
# Install one afterwards: nool admin plugin install policy-src/<name>/<name>.toml --policy
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test -p emb-rules --quiet
if [ "$#" -eq 0 ]; then set -- $(ls policy-src); fi
for name in "$@"; do
  nool admin plugin build "policy-src/$name"
done
