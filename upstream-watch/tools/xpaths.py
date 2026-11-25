"""xcl tree -> compat layout: crates/xcl -> crates/vilocity-compat; names with xcl/victory -> vilocity; drop .github and root Cargo files."""
import os, shutil, sys
for top in sys.argv[1:]:
    for n in (".github", "Cargo.toml", "Cargo.lock", ".gitignore"):
        p = os.path.join(top, n)
        if os.path.isdir(p): shutil.rmtree(p)
        elif os.path.exists(p): os.remove(p)
    c = os.path.join(top, "crates", "xcl")
    if os.path.isdir(c):
        os.rename(c, os.path.join(top, "crates", "vilocity-compat"))
    for root, dirs, files in os.walk(top, topdown=False):
        for n in dirs + files:
            m = n.replace("xcl", "vilocity").replace("victory", "vilocity")
            if m != n:
                os.rename(os.path.join(root, n), os.path.join(root, m))
