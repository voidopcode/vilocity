"""Makes upstream apps/vilocity-native/src/main.rs look like our lib.rs (in each given tree)."""
import os, sys
HEAD = """//! `vilocity native`: the native engine's command line, as a library so that
//! `vilocity.exe` can run it too (`vilocity-native.exe` is `src/main.rs`).
//!
//! Every command reads the user's own files and writes only to stdout or to
//! the user's cache directory (outside the repository).

"""
for top in sys.argv[1:]:
    p = os.path.join(top, "apps", "vilocity-native", "src", "main.rs")
    if not os.path.exists(p):
        continue
    s = open(p, encoding="utf-8", newline="").read()
    assert s.count("fn main() -> Result<()> {") == 1 and s.count("Cli::parse()") == 1, p
    s = s.replace("fn main() -> Result<()> {", "/// Runs the command line `args` (the first one is the program name).\npub fn main(args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<()> {")
    s = s.replace("Cli::parse()", "Cli::parse_from(args)")
    if not s.startswith("//!"):
        s = HEAD + s
    os.remove(p)
    open(os.path.join(os.path.dirname(p), "lib.rs"), "w", encoding="utf-8", newline="").write(s)
