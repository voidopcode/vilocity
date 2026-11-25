<p align="center">
  <img src="assets/logo.svg" width="160" alt="Vilocity logo">
</p>

<h1 align="center">Vilocity</h1>

<p align="center"><b>Xbox Series X|S games you own, running natively on Windows.</b></p>

<p align="center">
  <a href="https://discord.gg/mWzP7ejTek"><img alt="Join the Vilocity Discord" src="https://img.shields.io/badge/Join%20the%20Discord-5865f2?style=for-the-badge&logo=discord&logoColor=white" height="40"></a>
</p>

<p align="center">
  <img alt="Status: not playable yet" src="https://img.shields.io/badge/GTA%20VI-in%20world%2C%20not%20playable-f5a623?style=for-the-badge">
  <img alt="Windows 10 / 11 x64" src="https://img.shields.io/badge/Windows-10%20%2F%2011%20x64-2f80ed?style=for-the-badge">
  <img alt="Direct3D 12, Shader Model 6.6" src="https://img.shields.io/badge/D3D12-SM%206.6-1fb9d8?style=for-the-badge">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2024-7c5cff?style=for-the-badge">
</p>

<p align="center">Progress updates, releases and help are on the <a href="https://discord.gg/mWzP7ejTek">Vilocity Discord server</a>.</p>

Runs Xbox Series X|S (GDK) games you own natively on Windows. There is no CPU
emulation: the game's x64 code runs in our process, Vilocity supplies the
console's system libraries, and the console's RDNA2 GPU code is recompiled to
Direct3D 12 shaders. Two engines, one executable:

| Engine | Folder | Best at |
|--------|--------|---------|
| **native** | `native/` | Grand Theft Auto VI. Shaders become regular host D3D12 shaders. |
| **compat** | `compat/` | Other GDK titles: Minecraft Preview, Unicorn Overlord, Fortnite, XDefiant. Shaders read the game's memory directly. |

No game files are in this repository. Everything is built from files you
supply at runtime.

## GTA VI progress

**Not playable yet.** The game boots, runs its menus at a steady 30 fps,
loads saves and gets into the world. The world is where the work is now.

| # | Stage | Status | |
|---|-------|--------|---|
| 1 | Boot and first-boot screens (accessibility, language, settings, legal) | ✅ works, 30 fps | ██████████ |
| 2 | Intro video with sound | ✅ works | ██████████ |
| 3 | Start screen, profile and saves | ✅ works | ██████████ |
| 4 | Loading into the world | ✅ works | ██████████ |
| 5 | The world renders | 🟡 prologue cutscenes and the first room play, 3-7 fps (about 210-270 ms a frame); some draws still dropped | █████░░░░░ |
| 6 | The picture | 🟡 no rainbow glitter, faces and rooms lit, jumpsuits their real orange, smooth edges (AMD's PC FSR); ray-traced light stands in as a constant; orange dashes from the tiled cascade shadows and blocky edges against blurred backgrounds remain | ███████░░░ |
| 7 | The world keeps running | 🟡 the loss 2-3 minutes in is fixed; a driver timeout some 7 minutes in remains (a longer GPU timeout is being tested) | █████░░░░░ |
| 8 | Playable frame rate (30 fps) | 🔴 not yet: the card's own work is 70-130 ms a frame and our submit thread's copies and copy-back 100-150 ms; the card no longer waits for the game's writes beside shader writes | ██░░░░░░░░ |
| 9 | Missions start to finish | 🔴 not yet | ░░░░░░░░░░ |

<!-- status:start (written by scripts/update-status.ps1 on every commit; edits here are overwritten) -->
**Latest test run:** 2026-11-25 18:01, game build Series 1.0.2.1

| Measure | Value |
|---------|-------|
| Best frame rate (menus included) | 31.0 fps |
| Median frame time | 118 ms |
| Last frame | 5.0 fps, 200 ms, 4823 draws, 601 dropped without a pipeline |
| Shaders that failed to translate | none |
| Host pipelines refused by the driver | 0 |
<!-- status:end -->

Latest: the rainbow glitter is gone. A dump of every render target of a
world frame showed it in the final gather's output, fed by the screen
probes' radiance, which `CS_ScreenProbes_Inject` filled from the ray
passes' stale buffers. That program now clears its outputs and gives the
probes the same constant ambient light as their resolve
(`VILOCITY_ZERO_OUT`); the orange jumpsuits lost their red tint with it.
Also ported this round: cube mip levels (DexNom), code tables after the
instructions (VIctory #2143: 16-18 KiB of local memory per thread on
NVIDIA), constant buffer views capped at 64 KiB (VItoriaBr), null constant
buffer writes (VIcky), and a 64-thread card compare (victory-fork #1014).

Next up: the tiled cascade shadows on 32-lane GPUs (the orange dashes and
~16 px steps; victory-fork #1002), our submit thread's copies of game
memory, the heaviest compute programs, and the driver timeout some 7
minutes in.

> [!WARNING]
> Runs used to end with the GPU device removed 2-3 minutes into the world.
> Out-of-range group-shared memory accesses, which an NVIDIA card does not
> survive, are now kept in range; the longest run since went past 7 minutes,
> into the first room. Pipelines the driver's shader compiler crashes on are
> remembered (`refused-pipelines.txt` beside the pipeline library) and not
> asked for again.

Tested on one machine so far: Windows 11 Pro, NVIDIA GeForce RTX 5080,
AMD Ryzen 9 9950X3D, 64 GB RAM.

## Requirements

- Windows 10/11 x64, a CPU with AVX2
- A Direct3D 12 GPU with shader model 6.6 (and mesh shaders for the GTA VI world)
- Rust (version pinned in `rust-toolchain.toml`), Windows SDK for DXC
- Your own copy of the game

Each engine's README ([native](native/README.md), [compat](compat/README.md))
lists the details.

## Build

```powershell
cargo build --release -p vilocity
```

That makes `target\release\vilocity.exe`. `cargo build --release` also builds
each engine on its own, `vilocity-native.exe` and `vilocity-compat.exe`.

## Play

```powershell
target\release\vilocity.exe play <game folder or exe>
```

`play` picks the engine: native for GTA VI (`game_scarlett_master.exe`),
compat for everything else. `--engine native|compat` overrides it. Each
engine's own commands are under `vilocity native ...` and `vilocity compat ...`.

### GTA VI first run

Translate every shader and build every pipeline ahead of time, so the game
does not stop to do it while playing (again after an update of the shader
translator):

```powershell
target\release\vilocity.exe native translate --cache --pipelines <game folder>\shaders.rpf <game folder>\game_scarlett_master.exe <game folder>\XboxBvhBuilder.dll
```

That fills the shader cache and the pipeline library. A second run skips what
is already there. The library belongs to the GPU and driver it was built on.

Then `scripts\play-gta6.ps1` starts the game with the frame-rate switches
measured so far and opens `scripts\monitor-gta6.ps1`: frame rate, memory, CPU
and GPU use, where each frame's time goes, and the log's warnings and errors
(logs in `%LOCALAPPDATA%\vilocity\logs`).

### Controls

| Keys | Pad |
|------|-----|
| Enter / Backspace | A / B |
| Arrows | D-pad |
| W A S D | left stick |

XInput pads work too. The full keyboard map is in
[`native/README.md`](native/README.md#getting-started).

### Sandbox

`play` runs the native engine in an AppContainer with no network at all
(loopback included) inside a job object. The game can read only its own
folder (and the FidelityFX folder, if set) and write only Vilocity's `cache`
and `data` folders. It can't touch your files, other programs, the clipboard
or system settings, and every process of it dies with `vilocity.exe`. GPU,
audio and input are not slowed. `--no-sandbox` runs the game as you instead.
`vilocity native run ...` and `translate` are not sandboxed (translate only
reads the game's files).

Data lives in `%LOCALAPPDATA%\Vilocity` (the compat engine's per-game folders
in `%LOCALAPPDATA%\Vilocity\compat`). Settings use `VILOCITY_*` environment
variables.

## Tests

```powershell
cargo test --workspace -- --test-threads=4
```

No game data is needed.

## Layout

- `apps/vilocity`: `vilocity.exe` (engine choice, both engines' helper processes)
- `native/`: the native engine's crates (`vilocity-core`, `-gpu`, `-shader`, ...) and `vilocity-native`
- `compat/`: the compat engine, crate `vilocity-compat`
- `scripts/`: GTA VI launcher and live monitor

One Cargo workspace at the root; one `target\` folder for everything.

## Licence

Vilocity is a modified version of GPL-licensed software, changed in November 2026.

The native engine is GPL-3.0-or-later (`native/LICENSE`), the compat engine
GPL-2.0-only (`compat/LICENSE`). `vilocity.exe` links both, which is fine for
your own use, but that binary cannot be redistributed under either licence.
