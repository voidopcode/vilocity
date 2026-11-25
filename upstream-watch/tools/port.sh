#!/bin/sh
# port.sh <commit> [target]: 3-way merges a VIctory (or $REPO) commit (parent -> commit), renamed, into native/ (or target); scratch in $WORK.
set -e
S="$(cd "$(dirname "$0")" && pwd)"
W="${WORK:-${TEMP:-/tmp}/vilocity-port}"; mkdir -p "$W"
V="${REPO:-$LOCALAPPDATA/Vilocity/upstream-watch/Marvisak-VIctory}"
c=$1
cd "$W"; rm -rf pb pt; mkdir pb pt
files=$(git -C "$V" diff --name-only "$c^" "$c")
for f in $files; do
  git -C "$V" cat-file -e "$c^:$f" 2>/dev/null && { mkdir -p "pb/$(dirname "$f")"; git -C "$V" show "$c^:$f" > "pb/$f"; }
  git -C "$V" cat-file -e "$c:$f" 2>/dev/null && { mkdir -p "pt/$(dirname "$f")"; git -C "$V" show "$c:$f" > "pt/$f"; }
done
py -3 -I "$S/rename_paths.py" pb pt
py -3 -I "$S/rename.py" pb pt >/dev/null
py -3 -I "$S/rename_escaped.py" pb pt >/dev/null
py -3 -I "$S/mainlib.py" pb pt
echo "== $c: $(git -C "$V" log -1 --format=%s "$c")"
py -3 -I "$S/merge3.py" pb pt "${2:-D:/Project/Vilocity project/native}"
