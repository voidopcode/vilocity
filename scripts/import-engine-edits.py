# scripts/import-engine-edits.py <vilocity tree>
# After scripts/import-engine.sh: the edits a freshly imported fork engine needs to be
# Vilocity's (the app as a library for vilocity.exe, and small fixes of the rename).
# Edits a fork does not have a place for are skipped and said.
import os, shutil, sys
root = sys.argv[1]
j = lambda *a: os.path.join(root, *a)

def edit(path, pairs):
    if not os.path.exists(path):
        print(f'  no {os.path.relpath(path, root)} in this fork'); return
    s = open(path, 'rb').read().decode('utf-8')
    nl = '\r\n' if '\r\n' in s else '\n'
    t = s.replace('\r\n', '\n')
    for old, new in pairs:
        if old not in t:
            print(f'  not found in {os.path.basename(path)}: {old[:50]!r}'); continue
        t = t.replace(old, new, 1)
    open(path, 'wb').write(t.replace('\n', nl).encode('utf-8'))

# 1. The app: the fork's sources, its main.rs as our lib.rs (our main.rs and manifest stay).
src, app = j('native', 'apps', '_coolfox_app'), j('native', 'apps', 'vilocity-native')
if os.path.isdir(src):
    for f in os.listdir(os.path.join(app, 'src')):
        at = os.path.join(app, 'src', f)
        if os.path.isdir(at): shutil.rmtree(at)
        elif f != 'main.rs': os.remove(at)
    for f in os.listdir(os.path.join(src, 'src')):
        at = os.path.join(src, 'src', f)
        if os.path.isdir(at): shutil.copytree(at, os.path.join(app, 'src', f))
        else: shutil.copy(at, os.path.join(app, 'src', 'lib.rs' if f == 'main.rs' else f))
    shutil.rmtree(os.path.join(app, 'tests'), ignore_errors=True)
    if os.path.isdir(os.path.join(src, 'tests')):
        shutil.copytree(os.path.join(src, 'tests'), os.path.join(app, 'tests'))
        for f in os.listdir(os.path.join(app, 'tests')):
            edit(os.path.join(app, 'tests', f), [('CARGO_BIN_EXE_vilocity")', 'CARGO_BIN_EXE_vilocity-native")')])
    shutil.rmtree(src)
edit(os.path.join(app, 'src', 'lib.rs'), [
    ('//! `vilocity`: the command-line front end.',
     "//! `vilocity native`: the native engine's command line, as a library so that\n//! `vilocity.exe` can run it too (`vilocity-native.exe` is `src/main.rs`)."),
    ('name = "vilocity",', 'name = "vilocity native",'),
    ('fn main() -> Result<()> {',
     '/// Runs the command line `args` (the first one is the program name).\npub fn main(args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<()> {'),
    ('Cli::parse()', 'Cli::parse_from(args)'),
])
# 2. `warm` starts itself again: with what stands before `warm` (`native`, in vilocity.exe).
prefix = '// What stands before `warm` on our own command line (`native`, in `vilocity.exe`).\n'
edit(os.path.join(app, 'src', 'warm.rs'), [
    ('        Command::new(exe)\n            .args(["warm", "--these", &list.join(",")])',
     '        Command::new(exe)\n            ' + prefix + '            .args(std::env::args_os().skip(1).take_while(|a| a != "warm"))\n            .args(["warm", "--these", &list.join(",")])'),
    ('    Command::new(&exe)\n        .args(["warm", "--these", &list.join(",")])',
     '    Command::new(&exe)\n        ' + prefix + '        .args(std::env::args_os().skip(1).take_while(|a| a != "warm"))\n        .args(["warm", "--these", &list.join(",")])'),
])
# 2b. Sandboxed and not, the driver keeps the pipelines apart: a list of what is made for each.
edit(os.path.join(app, 'src', 'warm.rs'), [(
    '    let done_file = dir.join("warm-done.txt");\n',
    "    // (Vilocity) The driver keeps a sandboxed process's pipelines apart from the\n"
    "    // others': a list of what is made for each.\n"
    '    let sandboxed = std::env::var_os("VILOCITY_SANDBOXED").is_some();\n'
    '    let done_file = dir.join(if sandboxed { "warm-done-sandbox.txt" } else { "warm-done.txt" });\n')])
# 3. The lock file and toolchain are two folders up here.
edit(j('native', 'crates', 'vilocity-gpu', 'build.rs'), [('let root = crates.join("..");', 'let root = crates.join("../..");')])
# 4. The device id: the name is eight bytes now.
edit(j('native', 'crates', 'vilocity-hle', 'src', 'modules', 'gameinput.rs'),
     [('id[..7].copy_from_slice(b"vilocity");', 'id[..8].copy_from_slice(b"vilocity");')])
print('edits done (one of the two warm.rs forms is always "not found")')
