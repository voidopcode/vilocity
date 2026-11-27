## What this changes

<!-- One or two sentences. Link the issue if there is one. -->

## Why

<!-- The bug or the gain. If it comes from another emulator or fork, name the project and the commit. -->

## Tested

- [ ] `cargo build --release -p vilocity` builds
- [ ] `cargo test --workspace -- --test-threads=4`: __ passed, __ failed, __ ignored
- [ ] Ran the game (needed when the change touches `native/crates` or `compat/crates` outside tests)

**PC:** CPU / GPU / driver / Windows

**Before and after** (same place in the game, `main` and this branch):

| | `main` | this branch |
|---|---|---|
| Frame rate | | |
| Picture | <!-- screenshot --> | <!-- screenshot --> |

## Not tested

<!-- What you could not check: other GPU makers, other places in the game, other games. -->

---

- [ ] No game files or download links in this pull request
- [ ] I agree that this change is offered under the licence of the folder it touches (see CONTRIBUTING.md)
