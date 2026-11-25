"""3-way merges base->theirs into ours, in place. Usage: merge3.py base theirs ours. Root-level files are only reported."""
import filecmp, os, shutil, subprocess, sys
base, theirs, ours = sys.argv[1:4]
def files(top):
    out = set()
    for r, d, fs in os.walk(top):
        for f in fs:
            out.add(os.path.relpath(os.path.join(r, f), top).replace("\\", "/"))
    return out
B, T = files(base), files(theirs)
def same(a, b):
    return os.path.exists(a) == os.path.exists(b) and (not os.path.exists(a) or filecmp.cmp(a, b, shallow=False))
for rel in sorted(B | T):
    b, t, o = (os.path.join(x, rel) for x in (base, theirs, ours))
    if same(b, t):
        continue
    if "/" not in rel:
        print("ROOT     ", rel); continue
    if same(b, o):
        if os.path.exists(t):
            os.makedirs(os.path.dirname(o), exist_ok=True); shutil.copyfile(t, o); print("theirs   ", rel)
        else:
            os.remove(o); print("deleted  ", rel)
        continue
    if not (os.path.exists(b) and os.path.exists(t) and os.path.exists(o)):
        print("ADD/DEL  ", rel); continue
    r = subprocess.run(["git", "merge-file", "--diff3", "-L", "ours", "-L", "base", "-L", "fork", o, b, t])
    print("CONFLICT " if r.returncode else "merged   ", rel, r.returncode or "")
