#!/usr/bin/env python3
"""Resolve linear-memory addresses (i32.const literals) to string contents.

Parses the wasm Data section segment-by-segment (LEB decoding, no deps) and
prints the string starting at each requested memory address. Release bundles
have no name section; the literals a function references (format strings,
module_path!(), panic Locations) identify it. See README.md.
"""
import sys
from mmap import ACCESS_READ, mmap


def read_leb(buf: mmap, i: int) -> tuple[int, int]:
    result = 0
    shift = 0
    while True:
        b = buf[i]
        i += 1
        result |= (b & 0x7F) << shift
        if not (b & 0x80):
            return result, i
        shift += 7


def find_data_section(buf: mmap) -> int:
    """Walk section headers to the Data section (id 11) contents start."""
    i = 8  # magic + version
    while i < len(buf):
        sec_id = buf[i]
        i += 1
        size, i = read_leb(buf, i)
        if sec_id == 11:
            return i
        i += size
    raise SystemExit("no Data section found")


def main() -> int:
    if len(sys.argv) < 3:
        print(f"usage: {sys.argv[0]} <bundle.wasm> <memaddr> [memaddr ...]", file=sys.stderr)
        print("       memaddr: decimal or 0x-hex linear-memory address", file=sys.stderr)
        return 2
    wasm = sys.argv[1]
    targets = [int(a, 0) for a in sys.argv[2:]]

    with open(wasm, "rb") as f:
        buf = mmap(f.fileno(), 0, access=ACCESS_READ)

    i = find_data_section(buf)
    count, i = read_leb(buf, i)
    segments: list[tuple[int, int, int]] = []  # (mem_base, file_off, size)
    for _ in range(count):
        flags, i = read_leb(buf, i)
        base = 0
        if flags & 1 == 0:  # active segment: init expr = i32.const LEB, end
            if buf[i] != 0x41:
                raise SystemExit(f"unsupported init expr at 0x{i:x}")
            i += 1
            base, i = read_leb(buf, i)
            if buf[i] != 0x0B:
                raise SystemExit(f"expected end opcode at 0x{i:x}")
            i += 1
        size, i = read_leb(buf, i)
        segments.append((base, i, size))
        i += size

    print(f"parsed {len(segments)} data segments", file=sys.stderr)
    for t in targets:
        hit = next((s for s in segments if s[0] <= t < s[0] + s[2]), None)
        if hit is None:
            print(f"mem {t} (0x{t:x}): not in any active segment (passive?)")
            continue
        base, off, _size = hit
        raw = buf[off + (t - base) : off + (t - base) + 100]
        print(f"mem {t} (0x{t:x}): {raw.decode('utf-8', 'replace')!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
