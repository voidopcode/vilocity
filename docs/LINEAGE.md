# Lineage

Where Vilocity's code comes from, and which upstream commits each engine carries.


Vilocity is built from two open-source projects, renamed:

- the **native** engine is VIctory (GPL-3.0-or-later, `native/LICENSE`),
- the **compat** engine is XCL by ivall (GPL-2.0-only, `compat/LICENSE`,
  <https://github.com/ivall/xcl>), now developed at
  <https://github.com/xcl-dev/xcl>. `compat/` follows it in full (at
  `568e7db`, 2026-11-25), including its opt-in per-lane wave64 form
  (`VILOCITY_PER_LANE=1`) and direct RDNA2 → DXIL translator
  (`VILOCITY_DIRECT_DXIL`); only its release-profile `debug = 2` is left
  out. Its BCPack decoder is a port of VIctory's, used under GPL-2.0-only
  with VIctory's author's permission (`compat/THIRD_PARTY.md`).

The native engine also carries the changes of fxceless's VIctory fork
(GPL-3.0-or-later, <https://github.com/fxceless/victory-fork>), merged on
2026-11-24 on top of Vilocity's own work: GTA VI rendering fixes (meshlet
culling, descriptor tables, light read-back, gathers at a mip level, rect
lists, depth/stencil twins), shader clean-up passes, raw title-memory reads,
the emulated ray/box and ray/triangle test, faster submission, and more HLE
coverage. Its switches are renamed `VICTORY_*` → `VILOCITY_*` (for example
`VILOCITY_RELAY=2`, `VILOCITY_LATE_WAIT=3`, `VILOCITY_SHADER_OPT=0`,
`VILOCITY_WAVE_WIDE=1`); each is described where the code reads it, and
`native/docs/fork-ray-tracing.md` is its ray-tracing write-up. On a GPU of
32-lane waves, a wave64 program the fork's forms would keep at 64 lanes
gets Vilocity's split form instead; the fork's retry ladder runs only if that
fails.

The fork's next eight commits (`3bff182` to `9376fd6`, 2026-11-24/25) were
merged on 2026-11-25: f64 instructions and half interpolation pairs (from
asdaasdsaddf's fork; zlib-rs now decodes the shader database), the view-heap
clamp and `v_permlane16`/`x16` (triippywhale's fork), the depth bounds test
and faster defaults, textures picked by index checked again after upload,
packed textures kept in the upload queue, the guard over every view of a
page (myokex's victory-amd `c6dc4d8`, after VIctory PR #115 by Spexivus;
the kernel-read fix by Joshua Odom), byte-copied descriptors mirrored, levels
on demand (`VILOCITY_LEVELS_ON_DEMAND=1`, off), and the cheaper descriptor
sync. The fork measured the first room rendering right at 250-380 ms a
frame. Its own shader-cache key and warm-up overlap the upstream ones below,
so those stay; the key now also covers every `VILOCITY_*` switch the
translator reads, and `translate --warm <file of shader hashes>` warms just
the shaders a run's log names.

Upstream VIctory up to `70564ae` (2026-11-24) was ported on 2026-11-25 with
the fork as the base: where both built the same thing, the fork's version is
kept, since it ran GTA VI further. Taken from upstream:

- the shader database's effects and the shader pack (`7389303`, `286e33f`),
  shader warm-up (`translate --cache`, `c47b529`, `819f622`) and the
  pipeline pre-build from the shader pack (`9c58a1c`);
- FidelityFX on the host and console paths, `ACCC` containers, pixel
  ordered-section inputs, lane permutes that respect EXEC (`0b35b7f`);
- lane masks read as numbers in compute (`8af0472`); discarded pixels kept
  as helpers, f16 results rounded as the MODE says, denormal flushes for
  min/max/med3, the host FMA probe (`77053a0`); bounded relative register
  moves (`e0034c2`); `VILOCITY_POKE` (`532cb7c`);
- upstream's instruction-semantics, control-flow, cube and size tests
  (`328eec1`, `4f57f46`, `75677cb`), adapted where this engine binds buffers
  raw.

From VIctory after `70564ae`, taken on 2026-11-25 for GTA VI: the known
build (Series 1.0.2.1) and its startup-archive check (`a3acc87`), the RPC
runtime's GUID calls (`7216ab7`), and a helper lane's `v_readfirstlane`
reading its own value (`0601955`, a DEVICE_HUNG some 200 s into the world),
the descriptor heaps held while mirrored (`628b95d`) and the dynamic slot
search across the cursor (`c1c3281`, `072fde0`). The fork already had its
own forms of `4ab20fc` and `4e60ea3`; `ed0dbfd` needs VIctory's copy
tracking, which the fork replaced. Also fixed here: BCPack's BC7
dictionary entry 0 (as xcl's `7881dbc`) and a clean-up pass that made a
join's value before its definition (two GTA VI compute programs).

Kept the fork's own version instead: 64-bit image atomics, the BVH
intersection, the view-stride table, join liveness, 16-bit I/O packing,
cube gradients, the title-memory directory (upstream's page table), heap
views, the rectangle geometry stage, depth twins, buffer-view windows, and
the frame-cost work (`a7fc47a`, `13091f6`, `27c3dc9`, `0f63757`,
`4051149`, `c75fdba`, `d71c388`, `683ad7a`, `13bbd6f`, `86a5ed6`,
`6d021fe`, `bacb06a`, `7000573`). One upstream test case is left out: in
compute shaders this engine takes a sample's bias as its level (no implicit
derivatives).

Each engine keeps its licence. `vilocity.exe` links both, which is fine for
your own use, but that binary cannot be redistributed under either licence.
