"""Second rename pass: old names right after a string escape (\\n, \\t, \\r, \\0)."""
import os
import re
import sys

RX = re.compile(r"(\\[ntr0])(XCL_|VICTORY_|XCL|Xcl|VIctory|Victory|xcl|victory)")
REP = {"XCL_": "VILOCITY_", "VICTORY_": "VILOCITY_", "XCL": "Vilocity", "Xcl": "Vilocity",
       "VIctory": "Vilocity", "Victory": "Vilocity", "xcl": "vilocity", "victory": "vilocity"}

assert RX.sub(lambda m: m.group(1) + REP[m.group(2)], r'"};\nXclOut') == r'"};\nVilocityOut'

for top in sys.argv[1:]:
    for root, dirs, files in os.walk(top):
        dirs[:] = [d for d in dirs if d != "target"]
        for f in files:
            if not f.endswith((".rs", ".md", ".py", ".ps1", ".toml")):
                continue
            p = os.path.join(root, f)
            with open(p, encoding="utf-8", newline="") as h:
                s = h.read()
            n, k = RX.subn(lambda m: m.group(1) + REP[m.group(2)], s)
            if k:
                with open(p, "w", encoding="utf-8", newline="") as h:
                    h.write(n)
                print(p, k)
