"""Renames victory paths under each dir: apps/victory -> apps/vilocity-native, victory* -> vilocity*."""
import os, sys
for top in sys.argv[1:]:
    a = os.path.join(top, "apps", "victory")
    if os.path.isdir(a):
        os.rename(a, os.path.join(top, "apps", "vilocity-native"))
    for root, dirs, files in os.walk(top, topdown=False):
        for n in dirs + files:
            if "victory" in n:
                os.rename(os.path.join(root, n), os.path.join(root, n.replace("victory", "vilocity")))
