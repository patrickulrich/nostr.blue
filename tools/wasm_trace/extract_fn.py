#!/usr/bin/env python3
"""Dump a wasm function's disassembly by index, optionally zoomed to an offset.

Prints every instruction line of the requested functions so `call N` targets
and `i32.const <addr>` literal references can be collected for
resolve_str.py. See README.md for the full workflow.
"""
import argparse
import subprocess
import sys


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("wasm")
    ap.add_argument("funcs", nargs="+", help="function indices, e.g. 588 17571")
    ap.add_argument("--around", help="only print within SPAN bytes of this offset (hex)")
    ap.add_argument("--span", type=lambda s: int(s, 0), default=0x800)
    args = ap.parse_args()

    want = {f"func[{f}]:" for f in args.funcs}
    around = int(args.around, 16) if args.around else None
    lo = (around - args.span) if around else None
    hi = (around + args.span) if around else None

    proc = subprocess.Popen(
        ["wasm-objdump", "-d", args.wasm], stdout=subprocess.PIPE, text=True, bufsize=1 << 20
    )
    assert proc.stdout is not None
    cur = None
    for line in proc.stdout:
        if " func[" in line and line[0] in "0123456789abcdef":
            parts = line.split()
            cur = parts[1].rstrip(":") if len(parts) > 1 else None
            if cur in want:
                print(f"===== {cur} =====")
            continue
        if cur in want:
            if around is not None:
                try:
                    off = int(line.split(":")[0].strip(), 16)
                except (ValueError, IndexError):
                    continue
                if not (lo <= off <= hi):
                    continue
            print(line.rstrip())
    proc.wait()
    return 0


if __name__ == "__main__":
    sys.exit(main())
