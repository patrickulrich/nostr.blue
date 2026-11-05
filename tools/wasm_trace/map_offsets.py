#!/usr/bin/env python3
"""Map module-relative wasm offsets (Chrome console frames) to function indices.

Chrome's `$funcN` is the wasm function index space (imports included) and
`0xOFFSET` is a module-relative byte address — both match wasm-objdump
numbering exactly. See README.md for the full workflow.
"""
import subprocess
import sys


def main() -> int:
    if len(sys.argv) < 3:
        print(f"usage: {sys.argv[0]} <bundle.wasm> 0xOFFSET [0xOFFSET ...]", file=sys.stderr)
        return 2
    wasm = sys.argv[1]
    targets = []
    for a in sys.argv[2:]:
        targets.append(int(a, 16))
    if not targets:
        print("no offsets given", file=sys.stderr)
        return 2

    best: dict[int, tuple[int, str]] = {}
    proc = subprocess.Popen(
        ["wasm-objdump", "-d", wasm], stdout=subprocess.PIPE, text=True, bufsize=1 << 20
    )
    assert proc.stdout is not None
    for line in proc.stdout:
        # function header lines look like: "027f0e func[582]:"
        if " func[" in line and line[0] in "0123456789abcdef":
            parts = line.split()
            if len(parts) < 2 or not parts[1].startswith("func["):
                continue
            try:
                off = int(parts[0], 16)
                fn = parts[1].rstrip(":")
            except ValueError:
                continue
            for t in targets:
                if off <= t:
                    cur = best.get(t)
                    if cur is None or off > cur[0]:
                        best[t] = (off, fn)
    proc.wait()

    for t in sorted(targets):
        off, fn = best.get(t, (0, "?"))
        print(f"0x{t:x} -> {fn} (func starts 0x{off:x}, +{t - off} bytes in)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
