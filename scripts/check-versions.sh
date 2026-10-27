#!/usr/bin/env bash
# The crate, the derive and the explorer are released together, at one
# version: the root Cargo.toml's. Fails when another place says otherwise —
# the crate's `=` pin of the derive, the explorer's package.json, the
# example's requirement — or, given a version (a release's tag, without its
# `v`), when that is not the version, or the changelog has no entry for it.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p' Cargo.toml)
[ -n "$version" ] || { echo "no version in the root Cargo.toml's [workspace.package]" >&2; exit 1; }

wrong=0
expect() { # what, found
  if [ "$2" != "$version" ]; then
    echo "$1 says ${2:-nothing}, and the root Cargo.toml $version" >&2
    wrong=1
  fi
}

expect "magic_logs' pin of magic_logs_derive" \
  "$(sed -n 's/^magic_logs_derive = { version = "=\([^"]*\)".*/\1/p' magic_logs/Cargo.toml)"
expect "explorer/package.json" \
  "$(node -p 'require("./explorer/package.json").version')"
expect "examples/fleet/Cargo.toml" \
  "$(sed -n 's/^magic_logs = { version = "\([^"]*\)".*/\1/p' examples/fleet/Cargo.toml)"

if [ $# -gt 0 ]; then
  [ "$1" = "$version" ] || { echo "the release is $1, and the root Cargo.toml says $version" >&2; wrong=1; }
  grep -q "^## $version — [0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}$" CHANGELOG.md \
    || { echo "CHANGELOG.md has no entry \"## $version — <date>\"" >&2; wrong=1; }
fi

[ "$wrong" = 0 ] && echo "every version is $version"
exit "$wrong"
