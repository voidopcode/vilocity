# Upstream port tools

Scripts used to bring upstream work into Vilocity (2026-11-24/25). Run them
with Python 3 (`py -3 -I`) and Git Bash; they only read the upstream clones.

- `port.sh <commit> [target]` – one VIctory commit (parent → commit) as a
  3-way merge into `native/` (or `target`), renamed to Vilocity first; needs
  the clone at `%LOCALAPPDATA%\Vilocity\upstream-watch\Marvisak-VIctory`.
  Scratch files go to `%TEMP%\vilocity-port` (or `$WORK`).
  Conflicts are left as diff3 markers (`<<<<<<< ours`, `||||||| base`,
  `>>>>>>> fork`, where "fork" is the upstream side).
- `rename.py`, `rename_escaped.py`, `rename_paths.py` – VIctory/XCL names to
  Vilocity (text, after string escapes, and paths). `rename.py` keeps
  fixed-size markers and upstream credit links as they are (`PROTECT`).
- `xpaths.py` – an xcl tree laid out as `compat/` (crates/xcl →
  crates/vilocity-compat).
- `mainlib.py` – upstream `apps/victory/src/main.rs` as our `lib.rs`.
- `merge3.py base theirs ours` – per-file 3-way merge of whole trees.
- `resolve.py` – resolve diff3 blocks by rule (`o`, `f`, `of`, `fo`, or a
  function).
- `fixinit.py` – upstream test initializers adapted to this engine's fields.

Rule for native (see the root README): the fork is the base; where upstream
and the fork built the same thing, the fork's version stays. Export trees
with `git -c core.autocrlf=false archive` or line endings differ everywhere.
