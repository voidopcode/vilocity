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
| 5 | The world renders | 🟢 the saved game loads into the open world and plays; 26 console programs that need 64-lane waves have no form for 32-lane cards yet and their draws are dropped | █████████░ |
| 6 | The picture | 🟢 shade, faces, skin, water and sky look as they should (the black shade, black arms, white loading screen and blocks on the water are gone); textures of what just came into view can appear late | █████████░ |
| 7 | The world keeps running | 🟡 the loss 2-3 minutes in is fixed; a driver timeout some 7 minutes in remains (a longer GPU timeout is being tested) | █████░░░░░ |
| 8 | Playable frame rate (30 fps) | 🟡 26-27 fps at 5,000 draws a frame and 17-19 fps at 10,000-11,000 (Ryzen 9 9950X3D, RTX 5080, as a Series S); it was 3-6 fps in the same place | ███████░░░ |
| 9 | Missions start to finish | 🔴 not yet | ░░░░░░░░░░ |

<!-- status:start (written by scripts/update-status.ps1 on every commit; edits here are overwritten) -->
**Latest test run:** 2026-11-27 05:55

| Measure | Value |
|---------|-------|
| Frame rate while playing | no frames of the world in this run (menus only) |
| Shaders that failed to translate | compute 5 |
| Host pipelines refused by the driver | 0 |
<!-- status:end -->

Latest: the launcher is new, and it sets the engine for the PC it runs on. A rail on
the left (Play, This PC, Options, Tools, Log), the game with its own picture and PLAY
(the game's pictures are read from your copy; none is in this repository), and the
picture to choose: 1080p (the game's Series S picture), 1440p or 4K (its Series X
picture for a screen of that size: sharper, about a quarter slower and more).
`scripts\play-gta6.ps1` reads the PC (the card's maker and video memory, the memory,
the processor's threads) and picks the worker threads, what is kept on the card and
whether textures' levels get memory on demand; `-Describe` says what it found and
set, `-As "nvidia,8,16,12"` shows what another PC would get. Measured on one PC only
(Ryzen 9 9950X3D, RTX 5080): the other sets are the ones other forks measured on
theirs, and say so in the script. A PC that has not played yet starts with a list of
the pipelines the game draws with (`assets/pipelines-used.txt`, names only), so they
are compiled while the game loads instead of when each is first seen. A live log
window can open with the game (`-LogWindow`, `scripts\log-gta6.ps1`).

Before that: the frame rate panel is new: the frame rate in the colour of how it plays,
the frame's time and draws, the processor, the card and memory with bars. It shows
whenever the game's picture is seen at that corner, whichever window has the keyboard,
and goes when another window covers the game or the game is minimised (F8 hides it).
A compute dispatch now waits for its pipeline instead of being dropped when the
pipeline takes longer than the frame's half second (Exopus, DoubleGhosty/cool_fox
2a1965f): a dropped one ran only once and left what it bakes wrong for the whole
scene (seen here: a shirt in blocks of black and white). Tried and not kept: the
game's own ray tracing runs on this engine with an RTX 5080, at 3-5 fps; the five
lighting programs in the wide form (the launcher's option, removed) drew black blocks
on water in daylight; keeping the game's leftover sky light (`VILOCITY_AMBIENT_LEFTOVER`)
put white blocks on hair and fences.

Earlier: moving through the world no longer drops to half the frame rate. Measured by
walking, then running through the day scene with scripted keys (Ryzen 9 9950X3D, RTX
5080, Series S picture, 6,300-8,700 draws a frame): 24-29 fps with no freezes, where
it was 11-18 fps with freezes of up to half a second. The cause was the game's own
page mapping: 848 ms of every second went into map and unmap calls (182 and 114
microseconds each), because the game's 6.1 GiB of pages came from one section, whose
lock Windows holds while it rebuilds large pages. They come from sections of 2 MiB
now (`VILOCITY_SECTION_MB`; RyanCase94's and CuteBooty's finding, fxceless/victory-fork
#1089 and #1274): 11 and 3 microseconds a call, 117 ms a second. Standing still was
fast before and hid it: tests move the character from now on. Also new:
- On a processor of two dies (a Ryzen 9) the game's threads and the frame's run on the
  die with the bigger cache, the engine's workers on the other (`VILOCITY_DIES=0`: not):
  34.8 ms a frame against 37.1 standing, switched every 10 s in one run.
- F11 is the whole screen and back; the launcher starts the game that way ("Full
  screen": a window shrinks the 1080p picture to fit the desktop, which softens it).
- The driver's pipeline file is begun anew when the translator or the driver changes
  (it had grown to 4.1 GB, written whole at every save; about 150 MB now), and
  `vilocity warm --used` builds the pipelines of the places you have played in ahead
  of each start, in the game's sandbox (about 3 ms each afterwards; the first making
  of one takes from tens of milliseconds to 15 s). The
  launcher's "Build all pipelines ahead" does every one the game has ever made.
- The launcher's NVIDIA Smooth Motion switch is tested (written and read back), and a
  program that holds video memory can be closed from the launcher's list.
- From other forks, each read line by line first: the shared lock of the write guards
  (`VILOCITY_PORT_GUARD=1`, off by default; DoubleGhosty/cool_fox c781843; about 1 ms
  a frame here while walking), 40 shader variants and a blending repair (DexNom, by
  way of ZigZagBoi1/ScarlettPort a1eea72a), and textures' levels given memory on
  demand taking effect (`VILOCITY_LEVELS_ON_DEMAND=1`; ScarlettPort 1deb0be4).
Not fixed yet: shade is too dark in daylight (the stand-in for the traced light),
water, and 30 fps is not held everywhere (20-30 while driving).

Before that: every key of the keyboard does something now (the number pad, the function
keys and the punctuation keys too; Ctrl crouches, E interacts, M is the map, T the
phone), and every key can be named in the keys file. The two pictures measured in
the same night street, with runs that take no keys (`VILOCITY_KEYS_ONLY=1`), on a
Ryzen 9 9950X3D and an RTX 5080: as a Series S 41-43 ms a frame (24 fps), 23 GB;
as a Series X 53-63 ms (17 fps), 26.5 GB, and visibly sharper with less noise. The
Series S stays the default; the launcher's Quality is the Series X. With the title's
own upscaler in place of AMD's PC one the Series S picture is slower (44-51 ms) and
shows white blocks. New: `VILOCITY_AMBIENT_FLOOR=<level>` is the least light the
stand-in for the traced light gives a surface; with 0.05 (the launcher's "Brighter
nights") an unlit street at night is seen, where it was black. The table above
counts only frames of the world, not menus.

Earlier: a launcher with a window. `Vilocity Launcher.cmd` opens it: it checks the
PC (video memory free and who holds it, memory, disk, the engine's build), shows
the game's own name and cover from your game folder, and has the options (picture,
frame rate, sharpening, upscaler, panel, sandbox) beside a PLAY button; the game's
state and frame rate show in it while it runs. One option writes NVIDIA's Smooth
Motion (the driver's frame generation, RTX 40 and 50) into the driver's profile for
`vilocity.exe` (the setting ids as in LooperSalty/AnyXboxX's launcher); reading the
driver is tested here, the write is not yet. The game's window is now titled with
the game's own name ("Grand Theft Auto VI | Vilocity 27 fps ...") and carries its
own logo, both read from its manifest and files in your copy; nothing of the
game's is in this repository. New: `VILOCITY_FSR_SHARPNESS=<0 to 1>` has the PC
upscaler sharpen (the title asks for none); off by default.

Earlier: checked on the new engine with a scripted run: sound plays (the engine's
own recording of what the title plays has sound from the moment the world loads),
a click takes the mouse (kept in the window, no cursor) and Esc gives it back, F8
hides and shows the HUD. On foot, Space jumps and Shift runs now. On NVIDIA cards
the launcher no longer runs the loop that keeps the card busy: a median of 35.9 ms
a frame against 37.6-39.9 ms at 3,500-6,500 draws (RTX 5080). New switch,
off by default: `VILOCITY_WAVE_WIDE=1` with `VILOCITY_WAVE_WIDE_ONLY=<shader
hashes>` (after Vice-Tay) runs the listed wave64 programs in the wide form; the
three fog-transmission programs and the two light-tiling ones translate and run
with it (6f225b641c5b0708, 5dc3b25986622949, 926c46bab3b86bd6, 369cbe906ea72764,
2cc4c197ee29e291), their cost a frame is not measured yet.

Earlier: the native engine is now cool_fox's (codec_d's VIctory fork, build
30e54b4, GPL-3.0-or-later), in place of our own tree. Ours was some 60,000 lines
behind the forks that draw shade right, and porting it over piece by piece did
not converge. Kept from Vilocity: the one executable, the sandbox (and its hold
on the mouse), the launcher, the monitor and the HUD. In the same saved game, on
a Ryzen 9 9950X3D and an RTX 5080: shade is lit (it was black), arms and faces
show, the loading screen is no longer white, the water has no blocks, and a frame
takes 37-39 ms at 5,000 draws and 52-58 ms at 10,000-11,000 (171-315 ms before).
Memory: the game's commit levels off near 24.7 GB over seven minutes (21.1 GB on
entering the world). All keys of a pad are on the keyboard now, and can be changed
in `%LOCALAPPDATA%\vilocity\data\keys.txt`. The launcher translates the game's
shaders ahead once per build, and uses more worker threads on 16 cores or more.
First speed work of our own, from a profile of the deck scene (2026-11-27): the
twelve threads that translate lists took one lock for each of some 20,000
descriptor slots a frame that changed their resource, and stood at it for 21% of
their samples; they hand those over in batches now, and a list call costs 5.4
microseconds where it cost 7.9. The frame is no shorter for it: no thread is
the limit by itself. What a frame waits for, by the engine's timeline: the
title's page-mapping calls (16-26 ms in the slow frames), the lock of the write
guards (34 ms a frame over all threads) and the card (15-29 ms a frame).
Tests: the engine's own suite passes, but for two of the indirect-argument tests
that expect `VILOCITY_INDIRECT_REST=1` (they pass with it). Not done: 30 fps in
busy places, and the 26 wave64 programs.

Earlier: daylight is no longer white. The title's average-luminance program
(`ComputeAverageDepthWeightedLuminanceFromHistogramCS`) has thread 0 write the
histogram's running counts to shared memory, then a barrier, then every bin reads
them; the barrier was translated as a fence without a wait, so on 32-lane cards
threads 32 and up read zeros, the scene's average came out 0 and the exposure sat
at its upper clamp (2^5.98): every outdoor frame white. Its barriers now wait
(`VILOCITY_GROUP_SYNC_FULL` list): mean brightness 254 -> 91-94, sea, sky and
trees visible. Also in: cool_fox 30e54b4's frame-estimated stand-in for the
ray-traced light (`ambient.rs`, after `CS_ScreenProbes_TemporalResolve`;
`VILOCITY_AMBIENT=constant:<level>` the old constant), R9G9B9E5 targets as
R11G11B10 (`VILOCITY_E5_TARGETS`), longer face bakes, helper lanes' own offsets.

Before that: the mouse is the game's after a click in its window (cursor held there
and hidden) and the desktop's again after Esc or another window in front; the
clip is done outside the sandbox, which Windows refuses it. From avatarmanz:
lane_uniform proven to a fixed point (the ray-tracing builder's last 3 programs
translate on 32-lane cards) and placed views keyed without their padding words.
From Op-lite/victory-scarlett: 16-bit views and loads keep their bits
(`VILOCITY_HALF_VIEWS`, `VILOCITY_D16_BITS`, on; facial animation), and its NaN
guard as `VILOCITY_NAN_GUARD=1`, off: on, daylight went white and stayed white.
From stellarr-studio/kelvaroq, off and measured flat here: `VILOCITY_WATCH_RESERVATION`,
`VILOCITY_DIRECT_CONSTANTS`.

Before that: menus, the start screen and the intro are no longer held to the title's
30 frames a second (`VILOCITY_MAX_FPS`, the launcher's default 0: no limit; n: at
most n): the start screen runs at about 700 fps, the intro animation at 95-100,
and the movie keeps its own pace.

Before that: the intro movie plays (the animated title sequence and the Rockstar
logo were black): movie frames the title writes with the processor get an upload
buffer of their own per command list (RyanLockwoood/VIctory-cool_fox-LockWood
8cc31ab3). From the same fork, the XMA audio fixes (rings that wrap instead of
writing past their end, frames that end at their data, voices that restart when
refilled), fence waits that really wait, and a 256 MB DirectStorage staging floor
(`VILOCITY_DSTORAGE_STAGING_MB`). Faces in the cell and diner cutscenes are lit
more often: textures uploaded before their bytes landed are uploaded again
(`VILOCITY_PAGE_WRITES`, fxceless/victory-fork e4245f3), and buffer views written
after the frame's pass are made at submit instead of reading zeros. The mouse
stays inside the game's window while it is in front.

Before that: loading into the world no longer stalls on texture reads. While the
title remaps its memory a texture is now read through the kernel, and only one
whose pages are not all there yet waits as before (`VILOCITY_TEXTURE_NO_WAIT=2`,
the launcher's default): the wait went from 140-700 to 3-19 ms a frame, and the
start screen, loading art, logo, prison cutscene and Lucia's hair all show
(run 2026-11-26 19:3x). The monitor window's title line no longer errors.

Before that: the start-up screens, menus and loading screens show again. On the
Series S profile the title presents two planes and puts its interface on the
second, which we never showed: everything before the world was black. The
second plane (read at the console's 72-byte stride, codec-d/cool_fox 1ad6e67)
is now blended over the first (`VILOCITY_PLANES=1`: the first alone). The HUD
shows only while the game's window is in front.

Before that: Lucia's hair shows, a long ponytail: the hair strand programs, left
out until now because they did not translate or drew streaks, run by default
(`VILOCITY_HAIR=0` leaves them out). The title's ray passes that still run walk
no structure: every node test answers "no hit" (`VILOCITY_BVH_NO_HIT`, from
fxceless/victory-fork 96ff135) instead of reading garbage for up to 65,535 steps
a wave: 71-85 ms against 85-117 ms a frame. The title's GPU-only view of its
24 GB reservation now shows the same pages as the CPU one (from xcl 94bfcf0;
the opening is still black until the characters show: not fixed yet). From
DexNom/VIctory: textures with mapped middle blocks are uploaded (they were taken
as not there), a staging overrun and a depth-pitch overlap fixed, layout tests
against AMD's addrlib (they pass). And a small in-game HUD on the right of the
game's window: frame rate, CPU, video memory and RAM (`VILOCITY_HUD=0`: none).

Before that: three picture fixes from the codec-d/cool_fox snapshot (the build
PortWatch verified at 25 fps): the shader translator dropped programs whose
descriptor read sat behind a lane condition at a join (their transmission
prefilter: black squares, broken eyes); decoration bakes are found by any stage
or by their targets and take every due texture first; and one oversized indirect
record no longer empties the draws after it. With them the "INMATE" print showed
in 4 of 4 cell shots (about 1 in 3 before), hair and lit skin in 5 of 5. And
Series S now renders at half resolution with the title's own upscaler
(`VILOCITY_RENDER_SCALE=0.5`, `VILOCITY_OWN_UPSCALER=1`, from
trustno1-coder/VIctory-NVIDIA): 84-90 ms against 95-117 ms a frame. The mouse
now turns the camera and its buttons aim and fire (see Controls).

Before that: title buffers from 64 KiB up now get copies in the card's memory, compared
there (`VILOCITY_CARD_BUFFERS_MIN_KB=64`, `VILOCITY_CARD_BUFFERS_MB=2048`, the
launcher's default; from fxceless/victory-fork c36971b, the setting
trustno1-coder/VIctory-NVIDIA reports 27-32 fps with): copying back 20-29 ms to
14-18 ms a frame, picture unchanged. The frame stays at about 86-93 ms: the card
is now the limit. Ported too, off for now: `VILOCITY_PARALLEL_REFRESH` and
`VILOCITY_SETTLE_ROOM` (fork ceb9e1f, 06702fe), 8-13 ms less preparing, but one of
two launches lost the window's frosting and the print, so they wait for more runs.

Before that: in about two launches of three Lucia's hair showed as a flat dark cap,
her neck black, and the jumpsuit had no "INMATE" print, already wrong in the
G-buffer's colour. The launcher let texture refreshes run while title memory
was being remapped (`VILOCITY_TEXTURE_NO_WAIT=1`); a refresh that met a remap
read nothing and the texture kept zeros for the rest of the run. It now waits
(`0`, as fxceless/victory-fork issue #923 found): hair in 4 of 4 cell captures,
the print in 2 of 4, at the same frame rate. The print still misses sometimes.

Before that: 16-bit typed buffer reads and writes through integer views
(`buffer_load_format_d16_*` on R16_UINT) were converted as half floats: the
face network that moves mouths read zeros. They now keep their bits (ported
from fxceless/victory-fork PR #927; a GPU test checks loads and stores).

Before that: on NVIDIA, compute groups laid out in two dimensions (32x2, 64x4)
gave half their threads the wrong id when a program read other lanes: the
card fills such groups in 16x2 tiles. Each wave now takes its block of ids
from a counter (fxceless/victory-fork PR #1261, ported; a GPU test checks five
group shapes). The launch-to-launch dark faces and missing INMATE print are
not fixed yet.

Before that: running the light-list program `CS_PrescalarizeLightLoop` as two
separate 32-thread halves (commit 3b62842) cut its card time from about 3 ms
to under 0.2 ms, but blacked out faces and walls in 16-pixel tiles in the diner
cutscene. Reverted; the faces are lit again with the old form.

Before that: the card memory kept for copies of the title's buffers (6 GB, of
which some 660 MB are used) counted as taken when the texture queue asked
for room, and 16,000 textures waited while the card was half empty. With
1.5 GB kept the "INMATE" print, the inmate's hair and the faces in the
prologue cutscenes all show, at the same 11 fps.

Before that: about half the launches used to fall into a slow mode (3-5 fps,
black faces, a bald dark head, red jumpsuits): there the title's card
memory defragmenter, `CS_DefragHeap_ComputeRelocations`, took 230 ms of card
time a frame. It is cleared now (`VILOCITY_ZERO_OUT`); four launches in a
row ran fast.

Before that: the launcher shows the game a Series S by default
(`VILOCITY_CONSOLE`, `-SeriesX` for the old one). The title then renders
fewer pixels and leaves out the ray-traced and hair-strand passes this
layer cannot run: about 11 fps instead of 3, rooms lit, the inmate with hair
and an orange jumpsuit.
Also: textures the title fills late are looked at again for longer.

Before that, the last orange dashes went. They were 16-pixel tiles the
tiled shadow programs never shaded, so they kept the "lit" clear value:
bright bars on heads and shirts by the cell window. A dump of the shadow
tile lists showed only 64 tiles of every 1024: the two programs that build
the lists (`CS_CalcRasterClassificationListCounts`,
`CS_WriteRasterClassificationResults`) clear their shared memory from one
thread, then pass a barrier that had lost its wait on the host, and the
clear wiped the other 15 waves' tile counts. They now wait at every barrier
like the shadow programs (`VILOCITY_GROUP_SYNC_FULL`), and the window light
falls on the head with a smooth shadow side. The glass of the cell window
is clear again with it. New GPU tests cover the patterns these programs use
on 32-lane cards, and the diagnostics build can dump buffers beside its
target dumps (`VILOCITY_DUMP_BUFFERS`). The same lost barrier waits in the
ped skinning and meshlet culling programs left a dark jagged hole that came
and went on the jumpsuit's shoulders, and in the hair tile programs a dark band across
the bald inmate's head and red instead of orange jumpsuits; those wait at
every barrier too now. So do the face programs (subsurface tiles, blend
shape normals, lips, eyes): faces in the prologue cutscenes are lit now
instead of black but for the eyes.

Before that, the shadow programs themselves got the same treatment: a lane
that left a loop early passed a barrier whose wait was dropped, and the
other half-wave read shared memory before it was written (the race TuxBox
found on GeForce). The tiled cascade shadow programs also translate exactly
(victory-fork #1002), so every shader of the game translates.

Before that, the rainbow glitter went. A dump of every render target of a
world frame showed it in the final gather's output, fed by the screen
probes' radiance, which `CS_ScreenProbes_Inject` filled from the ray
passes' stale buffers. That program now clears its outputs and gives the
probes the same constant ambient light as their resolve
(`VILOCITY_ZERO_OUT`); the orange jumpsuits lost their red tint with it.
Also ported this round: cube mip levels (DexNom), code tables after the
instructions (VIctory #2143: 16-18 KiB of local memory per thread on
NVIDIA), constant buffer views capped at 64 KiB (VItoriaBr), null constant
buffer writes (VIcky), and a 64-thread card compare (victory-fork #1014).

Next up: the jumpsuit colour that changes from run to run, our submit
thread's copies of game
memory, the heaviest compute programs, and the driver timeout some 7 minutes
in.

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

Double-click `Vilocity Launcher.cmd`: pick the game folder, look at what the
check of the PC says, press PLAY. Without the window:
`scripts\play-gta6.ps1 -Game <game folder>` starts the game with the engine's
measured settings and opens `scripts\monitor-gta6.ps1`: frame rate, memory, CPU
and GPU use, where each frame's time goes, and the log's warnings and errors
(logs in `%LOCALAPPDATA%\vilocity\logs`). Once per build it first translates
every shader of the game ahead (about a minute on 16 cores), so that none is
translated while playing; the first session still stutters while the graphics
driver builds its pipelines.

### Controls

An Xbox controller works. So do keyboard and mouse: a left click in the game's
window takes the mouse (it turns the camera, and the cursor is hidden and kept
in the window); Esc or left Alt gives it back.

| Keys | Pad |
|------|-----|
| W A S D | left stick: move (driving: W accelerate, S brake, A / D steer) |
| Mouse, I J K L or the number pad's 8 4 2 6 | right stick: camera |
| Left / right mouse button (or O / U) | RT / LT: fire / aim |
| Space or Enter | A: jump, climb (driving: Shift or Enter) |
| Ctrl, R or Backspace | B: crouch, back |
| X or CapsLock | X: melee |
| F, E or Y | Y: interact, get in and out |
| Tab or mouse 4 | LB: holster |
| Q or mouse 5 | RB: cover (driving: Space or Q, the handbrake) |
| Shift or Z | left stick click: run (driving: H or Z) |
| C, H or middle click | right stick click: look behind |
| Arrows or 1 2 3 4 | D-pad (T: up, the phone; N: down; [ and ]: left and right) |
| Esc or P | Menu |
| V or M | View: the map |
| G | switches between the on-foot and the driving keys |
| F11 | the whole screen, and back to a window |

Every other key does something too (the function keys, the number pad, the
punctuation keys, Home, End, Insert, Delete and the rest each press a pad control),
except G, F8 (the frame rate panel), F11 (the whole screen), Left Alt (gives the
mouse back) and the keys Windows keeps for itself.

The keys are in `%LOCALAPPDATA%\vilocity\data\keys.txt` (made on the first
start): change them there and save, and the running game takes them within a
second. `VILOCITY_MOUSE_SENSITIVITY` (1.0) sets the mouse's speed. (Keys and
mouse are proneonn/Vertex's, by way of cool_fox.)

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

## Credits

Vilocity stands on other people's emulators and forks. Thank you to all of them.

**The two engines**

| Project | Licence | What Vilocity uses |
|---------|---------|--------------------|
| [VIctory](https://github.com/Marvisak/VIctory) by Marvisak | GPL-3.0 | the native engine's origin: loader, system libraries, GPU layer, shader translator |
| cool_fox by codec_d (build 30e54b4) | GPL-3.0 | the native engine today: imported whole and renamed (`native/`) |
| [XCL](https://github.com/xcl-dev/xcl) | GPL-2.0 | the compat engine (`compat/`), and the console instruction decoder in `vilocity-loader` |

**Forks we took fixes, ports or settings from**

| Project | What came from it |
|---------|-------------------|
| [fxceless/victory-fork](https://github.com/fxceless/victory-fork) | merged into the native engine; card copies of title buffers, parallel refresh and settle room |
| [avatarmanz/VIctory-7900XTX](https://github.com/avatarmanz/VIctory-7900XTX) | the frame-rate unlock, lane fixes, placed views |
| tippythetaidum | the face fix (victory-fork #927) |
| [proneonn/Vertex](https://github.com/proneonn/Vertex) | keyboard and mouse |
| [RyanLockwoood/VIctory-cool_fox-LockWood](https://github.com/RyanLockwoood/VIctory-cool_fox-LockWood) | the intro movie upload fix, XMA audio fixes, real fence waits, DirectStorage staging floor |
| [DexNom/VIctory](https://github.com/DexNom/VIctory) | texture and descriptor fixes |
| [trustno1-coder/VIctory-NVIDIA](https://github.com/trustno1-coder/VIctory-NVIDIA) | Series S at half resolution with the title's own upscaler, card buffer settings |
| [Belobug/VIctory-Belobug-AMD](https://github.com/Belobug/VIctory-Belobug-AMD) | the ambient light estimate, pipeline fixes |
| [uiliannn/VItoriaBr](https://github.com/uiliannn/VItoriaBr) | the launcher's 1080p preset, the loop limit, capped constant buffer views |
| triippywhale | command list fixes |
| RyanCase94, CuteBooty and KainanP (fxceless/victory-fork #1089, #1274; VIctory #3177) | the game's pages in sections of 2 MiB: no more waits behind Windows' large-page worker |
| [DoubleGhosty/cool_fox](https://github.com/DoubleGhosty/cool_fox) | the shared lock of the write guards (renderer port 6, c781843) |
| [ZigZagBoi1/ScarlettPort](https://github.com/ZigZagBoi1/ScarlettPort) | 40 shader variants and a blending repair (from DexNom), levels on demand taking effect |
| [LooperSalty/AnyXboxX](https://github.com/LooperSalty/AnyXboxX) | the launcher's NVIDIA Smooth Motion switch (the driver settings it writes) |

**Runtime parts, not shipped with Vilocity:** AMD FidelityFX (FSR) if you download it, and DXC
from the Windows SDK.

The compat engine's own outside sources (AnyPS5, FFmpeg, AMD ISA and addrlib, LLVM test data
and more) are listed with their licences in [`compat/THIRD_PARTY.md`](compat/THIRD_PARTY.md).

## Licence

Vilocity is a modified version of GPL-licensed software, changed in November 2026.

The native engine is cool_fox by codec_d (build 30e54b4), a fork of VIctory by
Marvisak, with work from fxceless/victory-fork, avatarmanz/VIctory-7900XTX (the
frame-rate unlock among it), tippythetaidum (the face fix, victory-fork #927)
and proneonn/Vertex (keyboard and mouse).

The native engine is GPL-3.0-or-later (`native/LICENSE`), the compat engine
GPL-2.0-only (`compat/LICENSE`). `vilocity.exe` links both, which is fine for
your own use, but that binary cannot be redistributed under either licence.
