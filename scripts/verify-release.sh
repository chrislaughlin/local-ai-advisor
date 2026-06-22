#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <version-or-tag>" >&2
  exit 2
fi

requested_version="${1#v}"
manifest_version="$({
  awk '
    /^\[package\]$/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && /^version = "/ {
      gsub(/^version = "|"$/, "")
      print
      exit
    }
  ' Cargo.toml
})"

if [[ -z "$manifest_version" ]]; then
  echo "could not read package version from Cargo.toml" >&2
  exit 1
fi

if [[ "$requested_version" != "$manifest_version" ]]; then
  echo "version mismatch: requested $requested_version, Cargo.toml has $manifest_version" >&2
  exit 1
fi

cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked

echo "release v$manifest_version verified"
