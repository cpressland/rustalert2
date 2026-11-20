#!/usr/bin/env python3
"""Fail if game files, or anything that looks like them, are in the repository.

Nothing from Red Alert 2 or Yuri's Revenge may be committed: not the game's
files, not files extracted from its MIX archives, not files converted from
them. This check runs in CI on every pull request. It looks at the files git
tracks and fails on:

- file types the game uses (MIX, SHP, VXL, AUD, BIK, CSF, maps, ...),
- the names of the game's own INI files (rulesmd.ini, artmd.ini, ...),
- any file whose SHA-256 matches a file of a real install
  (tools/game-data-hashes.txt; hashes are not content).

Test fixtures must be synthetic, written by our own encoders at test time.

    tools/game_data_guard.py                    # check the repository
    tools/game_data_guard.py --generate DIR     # rebuild the hash list from an install

Stdlib only; run it with `python3 -I`.
"""
import hashlib
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
HASHES = os.path.join(HERE, "game-data-hashes.txt")

# Extensions of the game's own data. Case-insensitive.
EXTENSIONS = {
    "mix", "mmx", "yro", "map", "mpr", "yrm", "shp", "vxl", "hva", "aud",
    "bag", "idx", "bik", "csf", "pal", "tmp", "tem", "sno", "urb", "des",
    "lun", "ubn", "fnt", "wav", "pcx", "sav", "exe", "dll",
}
# Names of INI files that ship inside the game's MIX archives, with or without
# the md (Yuri's Revenge) suffix.
INI_NAMES = re.compile(
    r"^(rules|art|ai|sound|eva|theme|battle|mapsel|missions|keyboard|"
    r"temperat|snow|urban|desert|lunar|urbann|mpmodes|coop|ra2|ui)(md)?\.ini$",
    re.IGNORECASE,
)
# Files over this size are hashed for --generate. Smaller ones are too likely
# to match something innocent.
MIN_HASH_SIZE = 1024


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def tracked_files():
    out = subprocess.run(["git", "ls-files", "-z"], check=True, capture_output=True).stdout
    return [p for p in out.decode().split("\0") if p]


def load_hashes():
    hashes = {}
    if os.path.exists(HASHES):
        for line in open(HASHES, encoding="utf-8"):
            line = line.strip()
            if line and not line.startswith("#"):
                digest, _, name = line.partition(" ")
                hashes[digest] = name.strip()
    return hashes


def check():
    known = load_hashes()
    problems = []
    for path in tracked_files():
        if not os.path.isfile(path):
            continue  # deleted in the working tree
        name = os.path.basename(path)
        ext = name.rsplit(".", 1)[-1].lower() if "." in name else ""
        if ext in EXTENSIONS:
            problems.append(f"{path}: .{ext} is a game file type")
        elif INI_NAMES.match(name):
            problems.append(f"{path}: has the name of one of the game's INI files")
        elif os.path.getsize(path) >= MIN_HASH_SIZE and sha256(path) in known:
            problems.append(f"{path}: identical to the game's {known[sha256(path)]}")
    if problems:
        print("Game data must not be committed (see LEGAL.md):", file=sys.stderr)
        for p in problems:
            print("  " + p, file=sys.stderr)
        return 1
    print(f"game-data guard: {len(tracked_files())} files checked, none from the game")
    return 0


def generate(root):
    rows = {}
    for dirpath, dirs, names in os.walk(root):
        # The player's own saves are theirs, not the game's: never list them.
        dirs[:] = [d for d in dirs if d.lower() != "saved games"]
        for n in names:
            p = os.path.join(dirpath, n)
            ext = n.rsplit(".", 1)[-1].lower() if "." in n else ""
            if ext != "sav" and (ext in EXTENSIONS or ext == "ini") and os.path.getsize(p) >= MIN_HASH_SIZE:
                rows[sha256(p)] = os.path.relpath(p, root).replace(os.sep, "/")
    with open(HASHES, "w", encoding="utf-8") as f:
        f.write("# SHA-256 of game files over 1 KiB, from tools/game_data_guard.py --generate.\n")
        f.write("# Hashes only: no game content. Used to refuse commits of game files.\n")
        for digest, name in sorted(rows.items(), key=lambda r: r[1].lower()):
            f.write(f"{digest} {name}\n")
    print(f"wrote {len(rows)} hashes to {HASHES}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--generate":
        sys.exit(generate(sys.argv[2]))
    if len(sys.argv) == 1:
        sys.exit(check())
    print(__doc__, file=sys.stderr)
    sys.exit(2)
