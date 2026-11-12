#!/usr/bin/env python3
"""Static checks on a Vita release build. Standard library only.

Checks the linked ELF (process parameters, metadata margin, absence of a given path
prefix), the SFO fields, and that the VPK holds the SELF byte for byte. Prints one line per
check and exits non-zero if any fails.

Usage: vita_release_check.py --elf F --self F --sfo F --vpk F --heap BYTES --forbid PREFIX
       [--sfo-field NAME=VALUE ...]
"""
import argparse
import hashlib
import struct
import sys
import zipfile

ELF_MARGIN = 3348  # SCE metadata slack between the RX end and the RW start


def elf_sections(data):
    if data[:7] != b"\x7fELF\x01\x01\x01":
        raise ValueError("expected a 32-bit little-endian ELF")
    (_, machine, _, _, phoff, shoff, _, _, phsize, phcount,
     shsize, shcount, shstrindex) = struct.unpack_from("<HHIIIIIHHHHHH", data, 16)
    if machine != 40:
        raise ValueError("expected an ARM ELF")
    loads = []
    for i in range(phcount):
        kind, _, vaddr, _, _, memsz, flags, _ = struct.unpack_from("<IIIIIIII", data, phoff + i * phsize)
        if kind == 1:
            loads.append((vaddr, memsz, flags))
    sections = [struct.unpack_from("<IIIIIIIIII", data, shoff + i * shsize) for i in range(shcount)]
    names = sections[shstrindex]
    blob = data[names[4]:names[4] + names[5]]
    named = {blob[s[0]:blob.index(b"\0", s[0])].decode(): s for s in sections}
    return loads, sections, named


def symbol_word(data, sections, named, wanted):
    """Value stored at a global data symbol, read through its section's file offset."""
    symtab, strtab = named.get(".symtab"), named.get(".strtab")
    if not symtab or not strtab:
        raise ValueError("ELF has no symbol table")
    names = data[strtab[4]:strtab[4] + strtab[5]]
    # Find the name once in the string table, then look for the symbol entry that points at it.
    key = b"\0" + wanted.encode() + b"\0"
    position = names.find(key)
    if position < 0:
        raise ValueError(f"symbol {wanted} not found")
    for off in range(symtab[4], symtab[4] + symtab[5], 16):
        name, value, size, _, _, shndx = struct.unpack_from("<IIIBBH", data, off)
        if name == position + 1:
            section = sections[shndx]
            return struct.unpack_from("<I", data, section[4] + value - section[3])[0]
    raise ValueError(f"symbol {wanted} not found")


def elf_margin(loads, named):
    rx_end = max(a + s for a, s, f in loads if f & 5 == 5)
    rw_start = min(a for a, _, f in loads if f & 6 == 6)
    init = named.get(".init_array")
    start = min(rw_start, init[3]) if init else rw_start
    return start - rx_end


def parse_sfo(data):
    magic, _, key_start, data_start, count = struct.unpack_from("<4sIIII", data, 0)
    if magic != b"\0PSF":
        raise ValueError("not an SFO")
    fields = {}
    for i in range(count):
        key_off, fmt, length, _, data_off = struct.unpack_from("<HHIII", data, 20 + i * 16)
        key = data[key_start + key_off:].split(b"\0", 1)[0].decode()
        raw = data[data_start + data_off:data_start + data_off + length]
        fields[key] = struct.unpack("<I", raw)[0] if fmt == 0x0404 else raw.rstrip(b"\0").decode()
    return fields


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--elf", required=True)
    p.add_argument("--self", dest="self_", required=True)
    p.add_argument("--sfo", required=True)
    p.add_argument("--vpk", required=True)
    p.add_argument("--heap", type=int, required=True)
    p.add_argument("--forbid", action="append", default=[])
    p.add_argument("--sfo-field", action="append", default=[])
    a = p.parse_args()

    failures = 0

    def report(ok, text):
        nonlocal failures
        failures += 0 if ok else 1
        print(f"{'PASS' if ok else 'FAIL'} {text}")

    elf = open(a.elf, "rb").read()
    loads, sections, named = elf_sections(elf)
    heap = symbol_word(elf, sections, named, "_newlib_heap_size_user")
    report(heap == a.heap, f"_newlib_heap_size_user={heap} expected={a.heap}")
    stack = symbol_word(elf, sections, named, "sceUserMainThreadStackSize")
    report(stack > 0, f"sceUserMainThreadStackSize={stack}")
    gap = elf_margin(loads, named)
    report(gap >= ELF_MARGIN, f"RX-to-RW margin={gap} bytes (minimum {ELF_MARGIN})")
    for prefix in a.forbid:
        hits = elf.count(prefix.encode())
        report(hits == 0, f"occurrences of the forbidden path prefix in the ELF={hits}")

    fields = parse_sfo(open(a.sfo, "rb").read())
    for key in sorted(fields):
        print(f"INFO sfo {key}={fields[key]}")
    for item in a.sfo_field:
        key, value = item.split("=", 1)
        report(str(fields.get(key)) == value, f"sfo {key}={fields.get(key)} expected={value}")

    self_bytes = open(a.self_, "rb").read()
    with zipfile.ZipFile(a.vpk) as z:
        members = sorted(z.namelist())
        print(f"INFO vpk members={members}")
        report(z.read("eboot.bin") == self_bytes, "vpk eboot.bin equals the SELF")
        report(z.read("sce_sys/param.sfo") == open(a.sfo, "rb").read(), "vpk param.sfo equals the SFO")
    print(f"INFO sha256 self={hashlib.sha256(self_bytes).hexdigest()}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
