# wasm_trace — offline symbolization of Chrome wasm console stacks

Turns `$funcNNNN @ file.wasm:0xOFFSET` frames from a browser console dump into
function indices, disassembly, and referenced string literals — WITHOUT a
repro, a debug build, or a name section. This is how the intermittent
`AlreadyBorrowed` panic in the `b2939062` smokes was root-caused offline
(writer: `nip65.rs` apply's `USER_RELAY_METADATA.write()`; holder:
`connection.rs` `user_read_relay_handles` peek-across-await).

## Requirements

- The **exact wasm bundle** the browser ran (content-hash in the console
  filename; dx artifacts live under `target/dx/nostrblue/.../assets/`).
- `wasm-objdump` (wabt) on PATH.
- Python 3.

## Workflow

### 1. Map stack offsets → function indices

Chrome's `$funcN` is the wasm function index space (imports included) and
`0xOFFSET` is a module-relative byte address — both match `wasm-objdump`
numbering exactly (verified across full stacks).

```sh
python3 tools/wasm_trace/map_offsets.py <bundle.wasm> 0x1954a61 0xd6e61 ...
# -> 0x1954a61 -> func[17571] (starts 0x195497d, +228 bytes in)
```

### 2. Dump a function's disassembly

```sh
python3 tools/wasm_trace/extract_fn.py <bundle.wasm> 588 17571
```

Optionally zoom to a window around a specific offset:

```sh
python3 tools/wasm_trace/extract_fn.py <bundle.wasm> 588 --around 0xd6e61 --span 2048
```

Look for `call N` targets and `i32.const <big>` values — the constants are
linear-memory addresses of literals (format strings, `module_path!()`,
panic Location strings).

### 3. Resolve memory constants → strings

Parses the Data section segment-by-segment (52k+ segments; simple LEB
parsing, no dependencies) and prints the string at each memory address:

```sh
python3 tools/wasm_trace/resolve_str.py <bundle.wasm> 5745556 5746572 ...
# -> mem 0x57ab94: 'nostrblue::stores::relay::nip65\x00...'
```

A `module_path!()` literal (e.g. `nostrblue::stores::relay::nip65`) names the
MODULE the panicking function was inlined from; adjacent log format strings
pinpoint the exact statement.

## Notes

- Release bundles have no name section — that's why Chrome shows `$funcN`.
  Function merging (`-IC=all`) can fold identical code, so a frame that
  appears in unrelated stacks is usually a merged helper; resolve by strings,
  not by index alone.
- Data-section layout: first segment's init expr is typically
  `i32.const 0x100000`; the parser handles the general active-segment case.
