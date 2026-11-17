"""Add or refresh one exact SHA-256 pin in tools/check_source.py.

Run this only for a reviewed source file that the audit rejects because it is
outside the general suffix rules. It writes one canonical line into
PINNED_FILES; it never grants a directory, extension or glob allowance.
"""
from pathlib import Path
import argparse
import hashlib
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "tools/check_source.py"
GAME_DATA_SUFFIXES = {
    ".pk3", ".pk4", ".bsp", ".tga", ".dds", ".wav", ".ogg", ".mp3", ".bik",
    ".roq", ".mpx", ".sav", ".elf", ".self", ".vpk", ".suprx", ".pvr",
    ".yuv", ".obj", ".bmp",
}
DIGEST = re.compile(r"^[0-9a-f]{64}$")


def pin_line(path, digest):
    if not DIGEST.fullmatch(digest):
        raise ValueError(f"not a SHA-256 digest: {digest!r}")
    return f'    "{path}": "{digest}",'


def add_pin(text, path, digest):
    """Return (new text, action); replace an existing path pin or insert a new one."""
    line = pin_line(path, digest)
    existing = re.compile(r'^    "' + re.escape(path) + r'": "[0-9a-f]{64}",$', re.M)
    match = existing.search(text)
    if match:
        if match.group(0) == line:
            return text, "unchanged"
        return text[:match.start()] + line + text[match.end():], "updated"
    marker = "    # Tooling, licence texts and test fixtures.\n"
    if path.startswith("configs/") and text.count(marker) == 1:
        return text.replace(marker, line + "\n" + marker, 1), "added"
    start = text.index("PINNED_FILES = {")
    end = text.index("\n}", start) + 1
    return text[:end] + line + "\n" + text[end:], "added"


def checked_path(value):
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or "\\" in value:
        raise ValueError(f"path must be repository-relative: {value}")
    if path.suffix.lower() in GAME_DATA_SUFFIXES:
        raise ValueError(f"refusing a game-data suffix: {value}")
    full = ROOT / path
    if not full.is_file() or full.is_symlink():
        raise ValueError(f"not an ordinary source file: {value}")
    return path.as_posix(), hashlib.sha256(full.read_bytes()).hexdigest()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    try:
        path, digest = checked_path(args.path)
        text = TARGET.read_text()
        new_text, action = add_pin(text, path, digest)
    except (ValueError, OSError) as error:
        print(f"pin_source_file: {error}", file=sys.stderr)
        return 1
    print(f"{action}: {path} {digest}")
    if not args.dry_run and action != "unchanged":
        TARGET.write_text(new_text)
    if not args.dry_run:
        print("Next: python3 tools/check_source.py --worktree && python3 tools/check_source.py")
    return 0


if __name__ == "__main__":
    sys.exit(main())
