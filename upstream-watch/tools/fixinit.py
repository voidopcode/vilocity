"""Adapts upstream HostShader/Parts initializers to this engine: memory fields -> root_indexed_views, split added."""
import os, re, sys
for top in sys.argv[1:]:
    for root, _, files in os.walk(top):
        for f in files:
            if not f.endswith(".rs"):
                continue
            p = os.path.join(root, f)
            s = open(p, encoding="utf-8", newline="").read()
            n = s
            n = re.sub(r"(\n(\s*)memory: [^\n]*,\n\s*memory_writes: [^\n]*,\n\s*view_addresses: [^\n]*,\n)(\s*heap: [^\n]*,\n\s*heap_writes: [^\n]*,\n)?",
                       lambda m: "\n" + m.group(2) + "root_indexed_views: Vec::new(),\n", n)
            n = re.sub(r"(\n(\s*)mesh_stride: [^\n]*,\n)(?!\s*split)", r"\1\2split: false,\n", n)
            if n != s:
                open(p, "w", encoding="utf-8", newline="").write(n)
                print(p)
