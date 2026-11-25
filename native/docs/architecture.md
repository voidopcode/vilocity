# Architecture

```
 user's files (outside repo)             Vilocity
 ───────────────────────────             ─────────────────────────────────────────
 game assets ──────────────────────────► vilocity-pipeline + vilocity-game
                                         (vilocity-formats, vilocity-shader)
                                                    │
                                                    ▼
                                         cache_dir: converted data
                                                    │
 game executable ─► vilocity-loader ────► title code (native x86-64)
                    prepare, map, bind      │ imports
                                            ▼
                                         vilocity-hle ── host Windows (shared APIs)
                                            │ D3D12.X
                                            ▼
                                         vilocity-gpu ─► host D3D12
                                            │ shaders at pipeline creation
                                            ▼
                                         vilocity-shader ◄─► cache_dir
```

Shaders are translated when the title creates a pipeline, from the bytes it
passes, wherever it loaded them from (executable, archive, memory).
Converting them ahead of time from files (`vilocity translate --cache`) is
only a warm-up: it fills the same cache, under the same key. With
`--pipelines` it also makes the host pipelines of the title's shader pack
into the pipeline library, from the creation streams the engine would pass
(`vilocity-game` builds them, `vilocity-gpu` reads them as it reads the
title's), so a run finds them under the names it gives them itself.

## Crates

| Crate | Role |
|-------|------|
| `vilocity-core` | config, asset-policy guard, logging |
| `vilocity-pe` | read-only PE32+ and import-library analysis |
| `vilocity-loader` | prepare, map, bind and start the title image |
| `vilocity-hle` | console API implementations, host pass-through policy |
| `vilocity-gpu` | D3D12.X objects, command lists and GPU memory → host D3D12 |
| `vilocity-shader` | Xbox shader containers → host shaders |
| `vilocity-formats` | the GPU's texture layouts (sizes, tiling, detiling), audio codecs; planned: BCPack |
| `vilocity-pipeline` | converter trait, content-addressed cache |
| `vilocity-game` | the title-specific formats, build identities, hooks |
| `apps/vilocity-native` | command-line front end |
| `xtask` | asset-policy check, git hooks |
| `tests/fixtures/hle-probe` | a stand-in "title" (our own code) for loader tests |

Everything except `vilocity-game` is platform-level and can be tested with
any Xbox Series title.

## Import resolution

For every `dll!function` the title imports, in order:

1. our implementation in `vilocity-hle::modules` (always wins);
2. the host's own function, if `HostPolicy` allows that DLL;
3. an export of a DLL the title ships beside its executable, mapped like
   the executable (only DLLs neither of the above serves);
4. a generated stub that names the import when called (`--unimplemented
   trap` breaks into the debugger and aborts; `zero` logs once and returns 0).

`LoadLibrary*` of a console-only DLL we implement (e.g. `xgameruntime.dll`)
returns a pseudo handle; `GetProcAddress` on it finds our functions.
`LoadLibrary*` of a title DLL returns its mapped image, mapping it first if
nothing imports it. By name, as by import, a title gets only host DLLs
`HostPolicy` passes through, and `GetProcAddress` on one returns our
override where import binding would.
`QueryApiImpl` hands out our runtime objects by provider and interface id
(`vilocity-hle::modules::xgameruntime` lists the areas); online areas answer
as an offline console. Game saves are files under `VILOCITY_DATA_DIR`.

Title storage is a drive on the console, and titles build paths on that.
`vilocity-hle::modules::files` binds every path-taking title import
(kernel32 and the C/C++ runtime) and rewrites paths on a virtual drive to
the host folder behind it; persistent local storage is one such drive over
a folder under `VILOCITY_DATA_DIR`. Other paths reach the host unchanged.

An exe's code reaches its static TLS as slot 0; mapped as a DLL it gets
another slot, so the loader redirects those loads (`vilocity-loader::tls`).
`vilocity run --execute` installs a crash reporter that names faults as
`module+rva` and unwinds the faulting thread's stack
(`vilocity-loader::crash`).

`vilocity census` shows this classification without running anything (it
does not map title DLLs; `vilocity run` without `--execute` does).

## Shaders

`vilocity_shader::ir::build` turns a native program into SSA, one
instruction per machine instruction. `vilocity_shader::structure` then
removes EXEC: lane masks become per-lane booleans, wave-wide mask tests
become per-lane branches, and vector instructions keep a guard only where
a lane can reach them with its EXEC bit clear. `vilocity_shader::range::build`
runs both, again where a bound on the index of a register-relative move
(`v_movrel*`) is proven on the per-lane form, so the move names only the
registers it can reach. `vilocity_shader::resource`
traces each descriptor and scalar memory address to the root parameter it
comes from (table, heap, root descriptor or constant). A descriptor a
waterfall loop reads from the first lane (`v_readfirstlane_b32`) is traced
to the per-lane load or choice that gives each lane its own (a mask's
`v_cndmask_b32`, or an if/else whose arms each set it: traced on the paths
the lanes take, from structurisation), and accessed with a non-uniform heap
index in one pass. `vilocity translate`
runs every shader it finds (containers, SGD5 files) through these and
lists what they cannot handle yet.

### Wave64 on 32-lane GPUs

A compute program that needs the console's 64 lanes (cross-lane reads,
masks read as numbers, LDS shared within one wave) declares
`WaveSize(64)`, which a GPU without 64-lane waves refuses (NVIDIA: 32..32).
On such a GPU (`D3D12_OPTIONS1.WaveLaneCountMax < 64`, found when the
host device is made) `vilocity-gpu` asks for the **split form**
(`emit::translate_for(.., true)`, cached apart from the other):

* The console wave runs as two 32-lane host waves of one group, its
  halves: `WaveSize(32)`. The host wave holding thread 0 (or, with fewer
  than 64 threads, the full one) is the lower half; console lane = half
  × 32 + host lane, and the thread id follows the console lane.
* Every lane runs every block, as on the console: console branches are
  taken by the whole wave (`…z` and `scc0` when the lanes' condition
  holds in every lane, the others when in some lane), and every vector
  instruction is guarded by the lane's full EXEC bit (`structure`'s
  `execs`), not by what reaching it leaves open.
* Where a value spans both halves the halves meet: each writes one word
  to group-shared memory, a group barrier, both read both words, another
  barrier. That covers masks read as numbers (ballots), `v_readlane`,
  `v_readfirstlane`, wave reductions and prefix sums, and branches on
  lane conditions. DPP, swizzles and `v_permlane(x)16` stay within 32
  lanes. Every LDS access is followed by a barrier.
* One console wave per group (up to 64 threads): the halves meet at
  group barriers, legal because both take the same branches.
* Several console waves per group: their branches may differ, so no group
  barrier. At the start (where every thread is) each host wave claims the
  next 32 console threads from a group-shared counter (a partial host wave
  the last ones), which gives its console wave and half. The halves of a
  console wave meet through arrival counters: each writes its word to
  slot k & 1 of its pair, fences, raises its counter to k + 1 and waits
  (bounded) until the other's is there. The other half is one exchange
  ahead at most, so it never rewrites a slot still being read. The wait
  is a loop, which ends the current block; masks read as numbers inside
  an instruction are therefore balloted before it (a first lowering pass
  finds which). This relies on the waves of a group making progress
  together, as GPUs run them (not necessarily WARP). The console
  compiler's own barriers stay; it leaves them out only within one wave.
* A small group's `v_readfirstlane` (outside waterfall loops) is split
  too: each host half would otherwise read its own first lane.

Tested on an RTX 5080 (`emit::split_tests`, groups of 48 to 256 threads,
one and two dimensions; a test holds back the upper halves to prove the
wait); wave reductions and prefix
sums are not covered by a GPU test there (the console compiler that
writes them for `diff_tests` is not available).

### Legacy geometry stages

A pipeline with a geometry shader runs on-chip legacy GS: the vertex
container's ES variant and the GS are one merged hardware program. They
become one host **mesh shader** (`emit::translate_geometry`; conventions
measured on retail programs by the compat engine):

* The ES code, then the GS code; the ES's `s_setpc_b64` (its handover)
  becomes a branch (`native::merged_geometry_code`).
* A group takes 21 triangles (`GeometryIo::PRIMITIVES`) in 64 threads.
  ES: lane l is corner l % 3 of triangle l / 3, its inputs in v9 on from
  the draw's input; it writes its vertex to LDS. GS: lane p is triangle
  p: v0 / v1 the LDS dword offsets of its vertices, s2 the output area,
  s3 vertices | triangles << 8.
* `s_sendmsg` emit / cut (in the lanes of EXEC) keep per-lane counts in
  group-shared memory; a lane's emitted vertices lie in LDS from the
  lowest byte it wrote in the GS code. Every lane ends in one epilogue:
  vertices copied out, strips turned into triangles (odd ones swapped),
  unused primitives culled.
* The draw's input is the root SRV `emit::GEOMETRY_INPUT` (63): dword 0
  the triangle count, from byte 16 one float4 per ES input row per
  triangle corner. `vilocity-gpu` matches each input to the title's input
  layout by semantic, gathers the corners on the CPU when the list is
  submitted (`primitive::gather_corners`), and dispatches a mesh group per
  21 triangles.

Limits: triangles in, triangle strips out; at most 12 vertices per GS
invocation (a mesh group's 256); one instance, triangle lists; vertices a
GPU pass of the same list writes are not seen. Tested with a synthetic
ES + GS (`emit::geometry_tests`) on an RTX 5080, not yet with the title's.

### The console BVH builder (`XboxBvhBuilder.dll`)

Its compute shaders (53, per the compat engine) need, beyond the title's
pack:

* Byte and halfword LDS and buffer access (done: a dword's bits, writes
  cleared and set atomically so neighbouring lanes keep theirs;
  `emit::subdword_tests`).
* Global stores and atomics on GPU addresses in root UAVs (done:
  `HostShader::root_uavs`, `u<slot>` in `ROOT_CBV_SPACE`; `global_store_*`
  of dwords, bytes and halves, `global_atomic_*` of 32 and 64 bits;
  `emit::root_uav_tests`). `vilocity-gpu` binds a root UAV to its
  buffer's host copy and declares the rest of that buffer GPU-written, so
  it goes back to title memory after the submission (a root view has no
  size).
* Root arguments that do not fit the user SGPRs: the console packs them
  into a table in memory at s[0:1] (root constants 4 bytes, root
  descriptors 8), which the builder's shaders read everything from. Not
  done: how the shader records describe those parameters is to be read
  from the builder's own shaders.
* Not done either: `global_atomic_inc` / `dec`, float min / max (the
  compat engine uses a compare-exchange loop), `flat_*`.
