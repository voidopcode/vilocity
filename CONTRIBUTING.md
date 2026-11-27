# Contributing to Vilocity

Thanks for helping. Pull requests are welcome from anyone: fork the repository,
make your change on a branch, and open a pull request against `main`. Only the
maintainer merges, after reading every line of it.

AI tools are welcome too. The rules are the same for code written by hand and
by an agent.

## Before you start

- **Small and focused**: one fix or feature per pull request. Big changes:
  open an issue or ask on [Discord](https://discord.gg/mWzP7ejTek) first.
- **Credit**: if your change comes from another emulator or fork, name the
  project and the commit, and keep its licence notice. It goes into the
  README's [Credits](README.md#credits).
- **No game data**: no files from a game package (executables, archives,
  textures, audio, video, `MicrosoftGame.config`, dumps), and no download links
  to them.

## Build and test

Set up the tools as in the README's [Quick start](README.md#quick-start-from-nothing-to-gta-vi), then:

```powershell
cargo build --release -p vilocity
cargo test --workspace -- --test-threads=4
```

All tests must pass. A change that fixes a bug comes with a test that fails
before it and passes after, where a test is possible.

A change to how the game runs or looks (anything under `native/crates` or
`compat/crates` outside tests) also needs a run of the game:

- the same place in the game with `main` and with your branch;
- the frame rate of both (the frame rate panel, F8, or the log in
  `%LOCALAPPDATA%\vilocity\logs`);
- a screenshot of both when the picture changes;
- your CPU, GPU, driver and Windows version.

A change that is faster but changes the picture, or that only helps one GPU
maker while breaking another, will not be merged as it is.

## Code style

- Follow the code around your change: its names, comments and structure.
- Do not reformat whole files (`cargo fmt` on files you did not otherwise
  touch), so the diff shows only your change.
- New behaviour that could cost speed or change the picture goes behind a
  `VILOCITY_*` switch first, with a one-line comment saying what it does and
  which run showed it helps.
- No new network access, no new processes, and no new dependencies without a
  reason in the pull request.

## Licence

`native/` is GPL-3.0-or-later ([native/LICENSE](native/LICENSE)) and `compat/`
is GPL-2.0-only ([compat/LICENSE](compat/LICENSE)). By opening a pull request
you agree that your change is offered under the licence of the folder it
touches.
