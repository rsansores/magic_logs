#!/usr/bin/env bash
# Sets the version of a release everywhere it is written — the root
# Cargo.toml, the crate's `=` pin of the derive, the explorer's
# package.json, the example's requirement — brings every lockfile along, and
# dates the changelog's unreleased entry. In the working tree only: the
# release commits it once every registry took the release.
#
#   scripts/set-version.sh <version> <date>
set -euo pipefail
cd "$(dirname "$0")/.."

version=${1:?the version to release}
date=${2:?the date of the release, YYYY-MM-DD}

sed -i "/^\[workspace.package\]/,/^\[/s/^version = \".*\"$/version = \"$version\"/" Cargo.toml
sed -i "s/^magic_logs_derive = { version = \"=[^\"]*\"/magic_logs_derive = { version = \"=$version\"/" \
  magic_logs/Cargo.toml
sed -i "s/^magic_logs = { version = \"[^\"]*\"/magic_logs = { version = \"$version\"/" \
  examples/fleet/Cargo.toml
node -e '
  const fs = require("fs");
  const file = "explorer/package.json";
  const json = JSON.parse(fs.readFileSync(file, "utf8"));
  json.version = process.argv[1];
  fs.writeFileSync(file, JSON.stringify(json, null, 2) + "\n");
' "$version"

for manifest in Cargo.toml examples/fleet/Cargo.toml; do
  cargo update --workspace --manifest-path "$manifest"
done

# The entry being released: one already dated for this version, or an
# unreleased one — `## Unreleased`, or `## <this version> — unreleased`,
# never another version's — which this dates.
escaped=${version//./\\.}
entry="^## \(Unreleased\|$escaped — unreleased\)$"
if ! grep -q "^## $escaped — [0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}$" CHANGELOG.md; then
  grep -q "$entry" CHANGELOG.md \
    || { echo "CHANGELOG.md has no \"## Unreleased\" or \"## $version — unreleased\" entry" >&2; exit 1; }
  sed -i "0,/$entry/s//## $version — $date/" CHANGELOG.md
fi

scripts/check-versions.sh "$version"
