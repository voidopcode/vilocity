#!/usr/bin/env bash
# scripts/import-engine.sh <fork source dir> <vilocity tree>
# Copies a VIctory fork's engine crates (cool_fox, Vice-Tay: crates/victory-*) over
# Vilocity's native ones, renamed. Afterwards: the app sources lie in
# native/apps/_coolfox_app (merge its main.rs into vilocity-native's lib.rs), and
# Vilocity's own edits are redone by hand (git show 46f0ecc lists them).
# References to other projects (owner/VIctory, VIctory-<fork>, victory-fork,
# "upstream VIctory", "VIctory <commit>") keep their name: they are credits.
set -euo pipefail
SRC="$1"; DST="$2"

rename_file() {
  perl -0pi -e '
    s{([A-Za-z0-9_]+/)VIctory}{$1\x01}g;
    s{VIctory(-[A-Za-z_][A-Za-z0-9_-]*)}{\x01$1}g;
    s{victory-(fork|scarlett|low-end-pc)}{\x02-$1}g;
    s{(upstream )VIctory}{$1\x01}g;
    s{VIctory( upstream)}{\x01$1}g;
    s{VIctory( (?:\#\d+|[0-9a-f]{7,}\b|issue|PR|pull))}{\x01$1}g;
    s{VICTORY}{VILOCITY}g; s{VIctory}{Vilocity}g; s{Victory}{Vilocity}g; s{victory}{vilocity}g;
    s{\x01}{VIctory}g; s{\x02}{victory}g;
  ' "$1"
}

copy_tree() { # <from dir> <to dir>
  rm -rf "$2"; mkdir -p "$2"
  cp -r "$1"/. "$2"/
  # File and folder names.
  find "$2" -depth -name '*victory*' | while read -r p; do
    mv "$p" "$(dirname "$p")/$(basename "$p" | sed 's/victory/vilocity/g')"
  done
  # Text files only.
  find "$2" -type f \( -name '*.rs' -o -name '*.toml' -o -name '*.md' -o -name '*.txt' -o -name '*.def' -o -name '*.c' -o -name '*.h' \) | while read -r f; do
    rename_file "$f"
  done
}

for c in core formats game gpu hle loader pe pipeline shader; do
  copy_tree "$SRC/crates/victory-$c" "$DST/native/crates/vilocity-$c"
done
copy_tree "$SRC/tests/fixtures" "$DST/native/tests/fixtures"
# The app: sources into a holding folder; the glue (lib.rs) is merged by hand.
copy_tree "$SRC/apps/victory" "$DST/native/apps/_coolfox_app"
echo imported
