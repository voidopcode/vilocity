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

## Quick start: from nothing to GTA VI

**1. Check your PC**
- Windows 10 or 11 (64-bit), a CPU with AVX2.
- A graphics card with Direct3D 12, shader model 6.6 and mesh shaders: NVIDIA
  RTX 20 series or newer, AMD RX 6000 series or newer. Ours is an RTX 5080.
- 32 GB of RAM or more (the game uses about 25 GB).
- Your own copy of GTA VI for Xbox Series X|S: the folder with
  `game_scarlett_master.exe` in it.

**2. Install the tools** (once)
- [Git](https://git-scm.com/download/win).
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  with the **Desktop development with C++** workload (it brings the Windows SDK too).
- [Rust](https://rustup.rs): run `rustup-init.exe` and keep the defaults.
  The right Rust version is picked by the repository itself.

**3. Get Vilocity and build it**
```powershell
git clone https://github.com/voidopcode/vilocity
cd vilocity
cargo build --release -p vilocity
```
The first build takes a few minutes. Yellow `warning:` lines are normal (in
PowerShell they can show in red as `NativeCommandError`): it worked when the
last line says `Finished`.

**4. Play**
1. Double-click **`Vilocity Launcher.cmd`** in the `vilocity` folder.
2. Pick your game folder. **This PC** shows what the launcher found (card,
   video memory, RAM); fix anything it marks in red. If another program holds
   video memory, **Close it** frees it.
3. Choose **1080p**, **1440p** or **4K** (1080p is the fastest) and press **PLAY**.

The launcher picks the engine's settings for your PC. The first start
translates the game's shaders ahead (about a minute on 16 cores), and the first
session stutters while the graphics driver builds its pipelines;
**Tools → Build all pipelines ahead** does that before you play instead.

**5. In the game**
- An Xbox controller works. With keyboard and mouse, click in the window to
  take the mouse; **Esc** or **Left Alt** gives it back. All keys are in
  [Controls](#controls).
- **F11**: full screen or window. **F8**: hides or shows the frame rate panel.

**Updating**: in the `vilocity` folder run `git pull`, then
`cargo build --release -p vilocity` again.

**Something wrong?**
- "Engine: not built yet": run the build in step 3, then press **Check again** in the launcher.
- Logs are in `%LOCALAPPDATA%\vilocity\logs`. Bring the newest one, with your
  card, driver and Windows version, to the
  [Discord server](https://discord.gg/mWzP7ejTek).

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
| 7 | The world keeps running | 🟢 twelve minutes of the opening and twelve in the open world, unattended, without a loss, an error in the log or a driver timeout (2026-11-27); longer play is not measured | ████████░░ |
| 8 | Playable frame rate (30 fps) | 🟡 33 fps at the median through the opening, 28-29 fps at 5,000 draws a frame and 25 fps at 9,000 in the open world (Ryzen 9 9950X3D, RTX 5080, as a Series S, medians over twelve minutes each); not 30 where the world is dense | ████████░░ |
| 9 | Missions start to finish | 🔴 not yet | ░░░░░░░░░░ |

<!-- status:start (written by scripts/update-status.ps1 on every commit; edits here are overwritten) -->
**Latest test run:** 2026-11-27 22:45

| Measure | Value |
|---------|-------|
| Frame rate while playing (typical) | 1.1 fps, 910 ms a frame |
| Best while playing | 3.5 fps |
| Last frame | 3.3 fps, 304 ms, 3822 draws, 159 dropped without a pipeline |
| Shaders that failed to translate | compute 7 |
| Host pipelines refused by the driver | 0 |
<!-- status:end -->

Latest: "will it work on this PC" in the launcher, clothes and faces drawn with another
texture's colours, the 4-7 fps drops explained and shown on the overlay, and what the
picture tests of 2026-11-27 evening ruled out.
- **"Translating the game's shaders" shows how far it is.** Two players took the step for
  stuck: it said "about a minute" (timed on a 16-core processor) and nothing moved. Its lines
  (one each 1,000 shaders) now go to `logs\shaders-progress.txt`, and the launcher shows the
  count, and from the second build on a bar against the last run's total. If it does stop,
  that file says where.
- **Launcher: save backups, a benchmark, Update, and a progress bar.** Tools now has: save
  backups (your saves are copied at each PLAY into `%LOCALAPPDATA%\vilocity\backups`, the
  newest 10 kept; Restore puts one back after copying the saves of now); a benchmark (starts
  the game and, once you are in the world, measures 60 seconds of play from the window's frame
  rate, then shows median, average, 5% low and highest with the PC's card, processor, memory
  and picture, and copies it to share); Update (git pull, then the engine is built, for copies
  made with git). Preparing the game's pipelines now also shows a progress bar.
- **The overlay reads AMD and Intel cards too.** It asked only nvidia-smi, so an RX 9070 XT
  showed "-%" and "- of - GB" (issue 3). Without nvidia-smi it now reads Windows' own GPU
  counters for the card the game draws on; on the RTX 5080 they agree with nvidia-smi
  (5.8 of 15.9 GB both).
- **The "VRAM leak" was other programs, and the overlay now names the biggest.** Over a
  10-minute 1440p session the game's card memory rose to 14.5 GB in the first minute, then
  stayed between 14.4 and 15.4 GB until it closed (what it takes it gives back): no leak.
  Other programs filled the rest: 5.3 GB was still held after the game closed, and one Python
  program alone held 3.9 GB of the 16 GB card. When the card is full the overlay now says
  which program holds the most: "CARD FULL: others 7.8 GB (python 3.9)".
- **8 GB cards: textures' levels on demand are no longer set.** The same scripted day walk
  (it ends swimming) on this 16 GB card held to about 8 GB, with the play script's 8 GB
  settings: with them 13 fps median and black objects, without them 32 fps median (lowest 16)
  and none seen. The switch is still there (`VILOCITY_LEVELS_ON_DEMAND=1`).
- **Cards under 14 GB (12 GB too) play the 1080p picture**, as those under 10 GB already did:
  1440p and 4K take about 13 GB of the card, and a full card ran at 4-7 fps. The launcher
  already said "yes, at 1080p" for them; now the play script holds to it.
- **The sandbox on a PC where Windows refused the engine its own cache folder** (issue 3, an
  RX 9070 XT on Windows 11): the engine stopped at its first step, "cannot resolve
  ...\cache\images: access denied", where the same check passes in the sandbox on the other
  PCs we have logs of. That check (it asks Windows for a folder's full path, to keep game data
  out of the source tree) now goes on with the path as written when Windows refuses, and the
  log says what was refused. If the sandbox is still refused something, the launcher now says
  so ("untick Sandbox in Options") instead of "did not start". Not tried on that PC yet.
- **DLSS that does not start no longer leaves the world black** (issue 4, an RTX 4080): NVIDIA's
  NGX did not initialize there, so DLSS never ran, but the game's own FidelityFX had already
  been made at the size DLSS needs, and every upscale was skipped: menus and HUD on a black
  world. Now a frame DLSS does not take is the game's colour scaled into the output by a pass
  of ours (the one the PC FidelityFX DLL falls back to), and the log says DLSS did not start.
  Tested here with DLSS made to fail, 1080p balanced, no PC FidelityFX DLL: before, black
  frames (HUD too, so the game's own FSR4 on that context is not the way); now the world at
  30 fps, softer than with DLSS.
- **Will it work on this PC.** The launcher's This PC page now begins with an answer:
  yes, yes at 1080p, it may with problems, or no, with the reasons, from what the game
  was measured to take: at 1080p about 22 GB of RAM and 7.5 GB of the card, at 1440p
  and 4K about 27 GB and 13 GB. No: under 24 GB of RAM, under 10 GB on the card, no AVX2,
  or a card without mesh shaders (GTX, older Radeon, Intel UHD). It may: 24-31 GB of
  RAM, a small page file (an RTX 5070 PC's log: 3.7 GB of commit free, textures held
  back), a card other than an NVIDIA RTX, Windows 10.
- **The launcher shows how far "Preparing the game's pipelines" is** ("1,200 of 3,517"):
  it looked stuck for minutes after an update. The play script keeps the step's lines in
  `logs\warm-progress.txt`; the step says how far it is every 100 pipelines.
- **Windows 10.** Its own Direct3D 12 stops at shader model 6.5, and the game's shaders need
  6.6. On Windows 10 the play script now fetches Microsoft's D3D12 Agility SDK 1.620.2 from
  nuget.org once (checked against its SHA-512 and Microsoft's signature) and keeps only
  D3D12Core.dll, with Microsoft's licence, in D3D12\ beside vilocity.exe (the repository may
  not pass it on). The engine makes its device on that runtime only where the system's lacks
  6.6 (Windows 10 22H2 from build 19045.3393 loads it); the sandbox may read the folder.
  Tested here by forcing it (`VILOCITY_AGILITY=1`): the device came from that runtime and the
  world ran at 20-31 fps once its pipelines were made (0-5 fps the first minute). Not tried
  on a Windows 10 PC.
- **The launcher's "will it work" is advice, not a gate**: an 8 GB card or under 24 GB of RAM
  says "it may, with problems" and PLAY stays on; only a processor without AVX2 or a card
  without mesh shaders stops it.
- **2 ms a frame: the write guards' lock rework is on by default** (renderer port #6, from
  cool_fox c781843, design by avatarmanz/VIctory-7900XTX; `VILOCITY_PORT_GUARD=0` turns it
  off). Two night walks with it switched every 4 s and every frame captured: 1.7 and 2.6 ms
  a frame faster (the slowest tenth 3 to 5 ms), torn frames 20 of 2,091 on against 31 of
  2,059 off. Tried and not kept: `VILOCITY_JOIN_PARTS` (3 to 5 ms faster, but torn frames
  went from 0.19 to 3.23 per 100).
- **Ctrl crouches on foot, as in GTA V** (the game's controls page: X is crouch; it was B).
  The keys file Vilocity comes with has it; an older keys file keeps its own keys until it
  is deleted (the game then writes the new one). F still gets in and out of cars, G
  switches to the driving keys. Not changed yet: in a car GTA V has E for the horn, H for
  the lights and Q for the radio; ours differ, and which buttons the game uses for those
  is still to be read from its controls page.
- **Measure** (launcher, Play page): runs the diagnostics build, whose log says where
  each frame's time and the memory go, for measuring from one's own play.
- **Striped clothes (a texture showing an old picture).** A texture picked by index is
  looked at again 4, 12, 40, 120 and 400 frames after its upload, and uploaded again if
  its memory changed. Textures some of whose pages the title also maps at other
  addresses are written there, unseen, at any time: in play logs of two PCs they were
  found changed up to 1,600 frames after the upload, past the last look, and kept the
  old picture from then on (all 183 changed ones of one session were of that kind).
  They are now looked at every 200 frames for as long as they are shown; the others
  keep the five looks. A 1440p walk: same frame rate (median 20 and 21 fps), no errors;
  no late change happened on that walk, so the fix is not seen at work yet.
- **A face drawn with a mask's colours (a "mosaic face").** While a texture waits for
  its host copy, its slot went on showing the texture it showed before if that one had
  the same size and format, and some 530 slots a second walking were given another
  texture of that kind, not the same one moved. Such a stand-in is kept now only if its
  memory starts as the new one's (the title moved it); else the grey stand-in shows for
  the frames until the texture is made. In a 1440p walk slots that kept another texture
  went from 128,563 to 31,756, at the same frame rate (median 20 and 21 fps). The mosaic
  itself was not seen in our runs, so it is not confirmed gone.
- **4-7 fps at the 1440p picture is the card's memory running out.** At 1440p the game
  holds 12.7-13.4 GB of the card. With about 6 GB held by other programs (browsers,
  Discord, a Python program), Windows cut the game's budget to about 12 GB and moved the
  rest to the PC's memory: the card's part of a frame went from about 6 ms to 64-122 ms.
  At 1080p the same kind of scene holds 7.6 GB. The overlay now says "CARD FULL: others
  X GB, try 1080p" in red when the card is over 93 % full. Walking and running the
  daylight town (RTX 5080): 24-28 fps at 1080p, 15-20 at 1440p.
- **Ruled out:** the fog's temporal pass for the floating blocks (left out, they stayed;
  they also show in the opening, and poles pass in front of them, so they are in the
  scene, not over it). "Shade in creases" on and off changed nothing visible on the
  opening's faces. The reflections stand-in on and off changed nothing visible in the
  opening or the daylight town; the glitter on skin (pool party) is in no test save yet.
- **Every setting, in the launcher and on F1, kept together.** The launcher's options
  have a list for each of the game's settings (upscaler, DLSS model, frame generation,
  light in the shade, shade in creases, brighter nights, fog light cap, reflections
  stand-in, heat haze, steadier picture, torn-frame fix), the picture buttons included.
  "Shade in creases" (live) is how strongly the light stand-in darkens corners and folds,
  faces included: its 0.6 m reach is the suspect for dark patches on faces in close-ups. They and the game's
  F1 menu read and write one file (`menu.txt` in the data folder), so each shows what
  the other changed; those marked `*` apply when the game starts (the play script reads
  them). New on F1 too: picture, DLSS model, heat haze, torn-frame fix (after
  trustno1-coder/VIctory-NVIDIA's menu). `VILOCITY_LEAVE_OUT=<shader hash>[,..]` leaves
  a shader out (from foxhound).
- **At most 60 frames a second by default**: menus and the loading screen ran at about
  1,000 and only heated the card (the launcher's "No limit" still has it).
- **Frame generation on Windows 11** (issue 2): Windows' GPU scheduling was taken for
  off when its registry value is not there, as on Windows 11 where it is on by default;
  the play script and the launcher now take it for on there.
- **A PC whose Direct3D 12 lacks shader model 6.6** (issue 3, an RX 9070 XT): the game
  stopped a few seconds in, after the runtime refused the shaders' root signatures. The
  engine now ends at its start saying so (Windows 11 has it, Windows 10's own may not),
  and the launcher's checks warn on Windows 10.
- **Measured walking and running through the town** (story save, RTX 5080, 1080p
  picture): 22 fps at the median (17 at the lowest) as the game is, 24 (19) with DLSS
  quality. Frame generation was refused until Windows is restarted after GPU scheduling
  is switched on.
- **The floating blurry blocks** at night (beside lit signs and lamps) are not fixed:
  not the heat haze, the light sprites, bloom, the lens and depth of field, nor the
  reflections' misses (each left out in turn, the blocks stayed). They grow over minutes;
  the suspect is the fog's tiled light, two of whose programs do not translate for 32-lane
  cards while the temporal pass behind them runs.
- **Settings inside the game: F1.** F1 shows a menu over the picture, drawn as the
  frame-rate panel is (its colours, Segoe UI, the bar in the colour of the frame rate,
  rounded corners); F1 or Esc hides it. Up and down pick, left and right change, and
  the change shows in the next frame and is kept at once (`menu.txt` in the data
  folder): light in the shade, brighter nights, the fog light's cap, the reflections
  stand-in, "steadier picture", a frame rate cap (30 to 60). Each card sees its own
  upscaler: on NVIDIA DLSS (off, quality, balanced, performance) and frame generation
  (off, 2x, 3x, 4x); on AMD and the others the game's own AMD FSR, no frame
  generation. The upscaler, and frame generation when it was off at the start, are
  marked `*`: they are kept now and used from the next start (the game's picture size
  is set when it loads); the play script reads them, and the launcher's DLSS boxes
  follow them. Frame generation needs Windows' hardware-accelerated GPU scheduling;
  the 3x and 4x of an RTX 50 are not checked here (it is off on the test PC). The last
  line puts everything back to the launcher's. While the menu is shown the game gets no
  keys and no mouse. F1 was the game's VIEW in the keys file; it is the menu's now.
  (After trustno1-coder/VIctory-NVIDIA's settings menu; its frame generation amounts
  are ported from it.)
- **First start on a PC that never ran the game** (issue 1, an RTX 5070 Ti): the first
  frame of the world waited while the shaders it uses were translated and compiled, one
  after the other, stood 30 s, and the game's own watchdog ended the run with a write to
  address 0. A frame's dispatches now wait for their pipelines 12 s at most
  (`VILOCITY_PIPELINE_COMPUTE_WAIT_MS`), for the workers that make them and not for a
  compile on the game's thread (one took 16 s here), and then within the usual half
  second; what a dropped one leaves wrong (a shirt in blocks of black and white) is
  right on the next start. Reproduced and checked here on the RTX 5080, with an empty
  cache and the shaders translated while playing: without the bound no frame for 52 s
  while 1,183 shaders were translated, then the same crash at the same address; with it,
  the bound reached once (12.9 s without a frame), no crash, and 21 to 29 fps a minute
  later.
- **Two faults of every NVIDIA card, fixed** (both from trustno1-coder/VIctory-NVIDIA,
  read line by line and ported with credit). Compute programs whose console threads
  span several host waves numbered them by the first thread of each host wave rounded
  down; NVIDIA fills groups of two dimensions 16 x 2 at a time, so two waves took the
  same numbers: on this RTX 5080 the test program wrote 32 of its 64 places, now all 64
  (`VILOCITY_THREAD_TICKET=0`: as before). And a list reset after the game replaced a
  view heap at the same address gave back slots of the new heap, which other draws then
  got as well. No change seen on the night street; all 292 translator and 371 engine
  tests pass.
- **Torn frames, what they are not.** Each of these was built and counted on the same
  116 s walk: a wait for the flights that write, through another host copy, memory a
  submission refreshes (as many torn frames, 12 ms a frame slower); a present that
  returns only when the frame before has landed (as many); the check of pieces fetched
  while a later flight writes them (`VILOCITY_FETCH_CHECK`) and the tracking of writes
  through views picked by index (`VILOCITY_PICKED_WRITES=2`): no clear change. What
  does remove them is fewer lists on their way to the card (`VILOCITY_LATE_WAIT_LISTS`,
  256): none in a walk at 64, for 6 ms a frame; 33 at 128. The launcher's "Steadier
  picture" stays the better trade (2 to 5 for 1.5 ms). The engine's own check
  (`VILOCITY_UPCOPY_CHECK=1`) shows the fault itself: older bytes of a copy's CPU side
  sent up over skinned vertices that shaders wrote, only with the steps done on the
  card. Why the CPU side lacks them is still open.
- **The card's memory, said where you play.** With other programs holding 6.5 GB of a
  16 GB card and the 1440p picture chosen, 14,000 textures waited at a time (blank
  eyes, wrong skin). The launcher's check now warns under 11 GB free (14 GB for the
  1440p and 4K pictures), and the frame-rate panel names what other programs hold from
  2 GB on.

Before that: DLSS on NVIDIA cards, every frame checked for faults, and what that found.
Measured on one PC (Ryzen 9 9950X3D, RTX 5080, 62 GB), with runs that take the
keyboard and whose every frame was captured and counted.
- **DLSS Super Resolution** takes the place of the game's own FSR upscale
  (louketa/foxhound's Streamline integration, ported): `scripts\play-gta6.ps1 -Dlss
  quality` (or `balanced`, `performance`), and a box in the launcher's Options.
  It needs NVIDIA's Streamline SDK unpacked at `%LOCALAPPDATA%\vilocity\streamline`
  (NVIDIA's download: nothing of it is in this repository; its library is loaded only
  if its signature checks, and never fetches anything). In six minutes of the opening
  at 1440p upscaled to 4K: 8,878 frames, all of them upscaled by DLSS, none failed,
  24 to 40 fps, inside the sandbox; at 1080p in the story save 24 to 28 fps, as without
  it. Frame generation has its switch (`-FrameGen`): Streamline refuses it while
  Windows' hardware-accelerated GPU scheduling is off (it is on the test PC), DLSS
  then runs alone and the launcher says what to switch on. Not seen running yet.
- **The character drawn torn.** Counting frames that differ from both their neighbours
  found the flicker: while he walks on the night street his body is pulled into long
  bands in single frames, 183 to 222 of 2,800 frames, in the version before this one
  too. It comes from the steps done on the card ahead of the flights
  (`VILOCITY_CARD_STEPS`): none at all with it off, at 58 ms a frame in place of 41.
  Without the joined copies and parts and with the game held until its frame is sent
  (`VILOCITY_SUBMIT_FRAMES=0`, now what the play script sets): 2 to 33 over five runs,
  for 2 ms a frame (42.7 against 40.7); 0 of 2,742 frames walking in daylight. It has a price in the opening: 32 ms a
  frame at the median in place of 29 (31 fps for 34), and its crowd scene still shows
  3 torn frames (6 before). The launcher's "Steadier picture" (`VILOCITY_SUBMIT_DEPTH=1`:
  the game at most one submission ahead) leaves 2 to 5 on that street for 1 to 2 ms
  more; it is off unless ticked, as the opening then runs at 29.7 fps for 31. Not
  closed: the fault itself is still in there.
- **Memory of the card.** Copies of textures nothing uses went only from 88 % of the
  card's budget on, 2 % under the line at which no new texture is made; when a scene
  loaded 7 GB of textures in half a minute thousands of textures waited (faces and skin
  without theirs) while 3 to 4 GB of copies whose game memory was long unmapped stayed.
  They go from 75 % on now (`VILOCITY_TEXTURE_KEEP_PERCENT`): in the opening's six
  minutes the card had no room once before, 321 s in, and never after, with the same
  324,000 uploads and the same frame rate.
- **Memory.** A view of memory the game placed is told apart by the fields its kind
  uses only (TaynanGT/Vice-Tay's fix, from kaultakaulta75-dev/Vice-Tay-7800X3D): 53
  million such views in five minutes are 33,690 shapes and one table entry, where the
  table grew for the whole run.
- **Keys.** The game's own controls page was read: A jumps and climbs, the left stick's
  click runs. Space (A) and Shift (the click) do that, seen in frames; G changes to the
  driving keys, where W accelerates (a motorbike ridden down the street).
- **The launcher.** STOP ends the play script with all it started (it ended only the
  step under way, and the game then started all the same); a second PLAY cannot start
  a second game (three started at once: two died while DLSS loaded); the steps before
  the game are named while they run; boxes for DLSS, frame generation and the game's
  ray tracing; a window of one size. `scripts\play-gta6.ps1 -PrepareOnly` does a start's
  one-time steps and comes back.
- The card Windows counts as high performance is taken on a PC with two
  (`VILOCITY_D3D12_ADAPTER=<index or name>` names another), and the stand-in light
  follows a change of place by the clock, not by frames (both after Vice-Tay). A
  stand-in for reflections whose rays find nothing (foxhound's) is in, off
  (`VILOCITY_AMBIENT_SPECULAR=1,1`): 150 pairs of frames with and without it showed no
  harm; the case it is for (a puddle under a roof mirrors the full sky, white with black
  squares, and the exposure falls) has not been compared yet.
- **1440p and 4K need the whole card.** Most of the card there is render targets of the
  size of a 4K screen, not textures: with other programs holding 3 GB of a 16 GB card
  it has no room from the first minute, under the old rule and the new one alike, and
  textures wait (a face with blank white eyes, seven minutes in). The launcher says so
  at those two choices. DLSS at the size of the output (`dlaa`) stops the game at its
  start and is not offered; balanced and performance run (1113x626 and 960x540 to
  1080p) and are no faster than quality: the frame is the processor's.
- **Twelve minutes unattended** (the opening to the cell, 1080p, as the play script
  sets it): no driver timeout, no error in the log, 33 fps at the median (30.0 ms; nine
  frames in ten under 36.9 ms), memory 22 GB at first, 33 GB at its most, 27 GB at the
  end. Faces in its close-ups are right (eyes, skin, soft shade). With another program
  holding 2.5 GB of the card the card still had no room now and then (17 of 79 looks):
  the launcher's check names such a program. Twelve more in the open world of the
  story save (mostly standing, three short walks): no error, the card never full,
  25 fps at the median (39.6 ms at 8,900 draws; nine frames in ten under 41.6 ms),
  memory 20 GB growing to 23.
- Known and open: with the pause menu up the blurred picture behind it drifts to green
  and magenta; the town of the current story save runs at 23 to 24 fps (9,000 to
  10,000 draws); the list of light programs in the wide form leaves grey rectangles on
  open water and is not set.

Before that: the launcher is new, and it sets the engine for the PC it runs on. A rail on
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
2cc4c197ee29e291), their cost a frame is not measured yet. **Warning, 2026-11-27:**
6871d6e4b3ac9a10 and 926c46bab3b86bd6 (the fog light, `CS_TransmissionVolumetrics`
and its underwater twin) in the wide form hung an RTX 5080 when the world was entered,
and the whole PC froze and restarted (the wide form's host waves wait for each other
by spinning). Do not list them; on NVIDIA leave this switch off.

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
| [louketa/foxhound](https://github.com/louketa/foxhound) (lucas-3080) | DLSS through NVIDIA's Streamline (56dba41), the stand-in for reflections |
| TaynanGT/Vice-Tay, by way of kaultakaulta75-dev/Vice-Tay-7800X3D | placed views told apart by the fields they use and a cap on views without a resource, the pick of the graphics card, the stand-in light that settles by the clock |

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
