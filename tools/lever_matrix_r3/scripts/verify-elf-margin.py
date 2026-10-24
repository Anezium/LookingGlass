#!/usr/bin/env python3
"""Verify the linked Vita ELF before installing its VPK. No third-party modules."""
import struct
import sys
from pathlib import Path


def verify(path):
    data = path.read_bytes()
    if data[:7] != b"\x7fELF\x01\x01\x01":
        raise ValueError("expected a 32-bit little-endian ELF")
    header = struct.unpack_from("<HHIIIIIHHHHHH", data, 16)
    (_, machine, _, _, phoff, shoff, _, _, phsize, phcount,
     shsize, shcount, shstrindex) = header
    if machine != 40 or phsize < 32 or shsize < 40:
        raise ValueError("expected an ARM ELF with program and section headers")
    loads = []
    for i in range(phcount):
        kind, _, address, _, _, memory_size, flags, _ = struct.unpack_from(
            "<IIIIIIII", data, phoff + i * phsize)
        if kind == 1:
            loads.append((address, memory_size, flags))
    rx = [address + size for address, size, flags in loads if flags & 5 == 5]
    rw = [address for address, _, flags in loads if flags & 6 == 6]
    if not rx or not rw:
        raise ValueError("missing RX or RW LOAD segment")
    sections = [struct.unpack_from("<IIIIIIIIII", data, shoff + i * shsize)
                for i in range(shcount)]
    strings = sections[shstrindex]
    names = data[strings[4]:strings[4] + strings[5]]
    init = [s for s in sections
            if names[s[0]:].split(b"\0", 1)[0] == b".init_array"]
    if len(init) != 1:
        raise ValueError("expected exactly one .init_array section")
    init_address, init_size = init[0][3], init[0][5]
    if not any(flags & 6 == 6 and address <= init_address
               and init_address + init_size <= address + size
               for address, size, flags in loads):
        raise ValueError(".init_array is outside the RW LOAD segments")
    end = max(rx)
    start = min(min(rw), init_address)
    gap = start - end
    print(f"{path}: RX end={end:#x}; RW start={min(rw):#x}; "
          f".init_array={init_address:#x}; gap={gap} bytes")
    if gap < 3348:
        raise ValueError(f"RX-to-RW/.init_array gap {gap} is below 3348 bytes")
    print("PASS linked ELF metadata margin >= 3348 bytes")
    return gap


def main():
    if len(sys.argv) != 2:
        raise ValueError("usage: verify-elf-margin.py <built-ELF-or-release-directory>")
    path = Path(sys.argv[1])
    if path.is_dir():
        Bs = [p for p in [path / "looking-glass", path / "looking-glass.elf"]
                      if p.is_file() and p.read_bytes()[:4] == b"\x7fELF"]
        if len(Bs) != 1:
            raise ValueError("expected one looking-glass[.elf] in the release directory; "
                             "pass the ELF path printed by cargo-vita explicitly")
        path = Bs[0]
    verify(path)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, IndexError, struct.error) as error:
        sys.exit(f"FAIL: {error}")
