"""Renames VIctory/XCL to Vilocity in the text files under the given folders.

Usage: py -3 -I rename.py <dir>...   (prints the files it changed)
"""
import os
import re
import sys

# Kept as is: fixed-size format markers, a stored legacy value, real links.
PROTECT = [
    'b"XCLTRAN1"', 'b"XCLTRAN2"', '"XCLO"', '"XCLQUEUE"',
    "XCL00000000000000000000000000000000000001",
    "discord.gg/xcl", "github.com/ivall/xcl", "github.com/xcl-dev/xcl", "github.com/Marvisak/VIctory",
    "github.com/fxceless/victory-fork", "Marvisak/VIctory",
]

NB = r"(?<![A-Za-z0-9])"  # no letter or digit before (an underscore is fine)

RULES = [
    # crates, executables and paths that get their own names
    (r"CARGO_BIN_EXE_xcl\b", "CARGO_BIN_EXE_vilocity-compat"),
    (NB + r"xcl\.exe", "vilocity-compat.exe"),
    (NB + r"victory\.exe", "vilocity-native.exe"),
    (NB + r"xcl::", "vilocity_compat::"),
    (NB + r"victory::", "vilocity_native::"),
    (r"apps/victory\b", "apps/vilocity-native"),
    (r"apps\\victory\b", r"apps\\vilocity-native"),
    (r"crates/xcl\b", "crates/vilocity-compat"),
    (r"crates\\xcl\b", r"crates\\vilocity-compat"),
    (r"-p victory\b", "-p vilocity-native"),
    (r"-p xcl\b", "-p vilocity-compat"),
    # everything else
    (NB + r"XCL_", "VILOCITY_"),
    (NB + r"VICTORY_", "VILOCITY_"),
    (NB + r"XCL", "Vilocity"),
    (NB + r"Xcl", "Vilocity"),
    (NB + r"VIctory", "Vilocity"),
    (NB + r"Victory", "Vilocity"),
    (NB + r"xcl", "vilocity"),
    (NB + r"victory", "vilocity"),
]
RULES = [(re.compile(p), r) for p, r in RULES]
TEXT = {".rs", ".toml", ".md", ".ps1", ".py", ".yml", ".yaml", ".txt", ".config",
        ".json", ".hlsl", ".xml", ".gitignore", ".gitattributes", ""}


def rename_text(s):
    for i, p in enumerate(PROTECT):
        s = s.replace(p, f"\0{i}\0")
    for pat, rep in RULES:
        s = pat.sub(rep, s)
    for i, p in enumerate(PROTECT):
        s = s.replace(f"\0{i}\0", p)
    return s


def main(dirs):
    for top in dirs:
        for root, subdirs, files in os.walk(top):
            subdirs[:] = [d for d in subdirs if d not in ("target", ".git")]
            for f in files:
                ext = os.path.splitext(f)[1].lower()
                if ext not in TEXT and not f.startswith("."):
                    continue
                path = os.path.join(root, f)
                try:
                    with open(path, encoding="utf-8", newline="") as h:
                        old = h.read()
                except UnicodeDecodeError:
                    continue
                new = rename_text(old)
                if new != old:
                    with open(path, "w", encoding="utf-8", newline="") as h:
                        h.write(new)
                    print(path)


def self_check():
    t = rename_text
    assert t("use xcl::host;") == "use vilocity_compat::host;"
    assert t("XCL_DATA_ROOT") == "VILOCITY_DATA_ROOT"
    assert t("is_xcl_copy") == "is_vilocity_copy"
    assert t("exclusive EXCLUSIVE") == "exclusive EXCLUSIVE"
    assert t("XClosedCaptionProperties") == "XClosedCaptionProperties"
    assert t('b"XCLTRAN1"') == 'b"XCLTRAN1"'
    assert t("victory_core::x") == "vilocity_core::x"
    assert t("VIctory runs") == "Vilocity runs"
    assert t("https://discord.gg/xcl") == "https://discord.gg/xcl"
    assert t("XCL.TestTitle") == "Vilocity.TestTitle"


if __name__ == "__main__":
    self_check()
    main(sys.argv[1:])
