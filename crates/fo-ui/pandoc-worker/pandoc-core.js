// FileOdyssey: run pandoc.wasm on one request frame and return a result frame.
// Frame layout (see crates/fo-core/src/pandoc/frame.rs): 5-byte magic,
// u32 LE header length, JSON header, then the payloads in header order.
//
// Instance set-up follows pandoc-wasm's core.js (MIT, Tweag I/O Limited,
// Johannes Wilm and contributors; see PANDOC-WASM-LICENSE.txt).

import { ConsoleStdout, Directory, File, OpenFile, PreopenDirectory, WASI } from "@bjorn3/browser_wasi_shim";

const enc = new TextEncoder();
const dec = new TextDecoder();

export function unpack(magic, buf) {
  const u8 = new Uint8Array(buf);
  if (u8.length < 9 || dec.decode(u8.subarray(0, 5)) !== magic) throw new Error("damaged frame");
  const n = new DataView(u8.buffer, u8.byteOffset).getUint32(5, true);
  const head = JSON.parse(dec.decode(u8.subarray(9, 9 + n)));
  return { head, body: u8.subarray(9 + n) };
}

export function pack(magic, head, blobs) {
  const h = enc.encode(JSON.stringify(head));
  const total = 9 + h.length + blobs.reduce((a, b) => a + b.length, 0);
  const out = new Uint8Array(total);
  out.set(enc.encode(magic), 0);
  new DataView(out.buffer).setUint32(5, h.length, true);
  out.set(h, 9);
  let at = 9 + h.length;
  for (const b of blobs) {
    out.set(b, at);
    at += b.length;
  }
  return out.buffer;
}

export function errorFrame(message) {
  return pack("FOPR1", { error: String(message) }, []);
}

/** Loads and starts one pandoc instance. `fs` is the root directory map. */
export async function createPandoc(wasmBytes) {
  const args = ["pandoc.wasm", "+RTS", "-H64m", "-RTS"];
  const root = new Map();
  const fds = [
    new OpenFile(new File(new Uint8Array(), { readonly: true })),
    ConsoleStdout.lineBuffered(() => {}),
    ConsoleStdout.lineBuffered(() => {}),
    new PreopenDirectory("/", root),
  ];
  const wasi = new WASI(args, [], fds, { debug: false });
  const { instance } = await WebAssembly.instantiate(wasmBytes, { wasi_snapshot_preview1: wasi.wasiImport });
  wasi.initialize(instance);
  instance.exports.__wasm_call_ctors();
  const view = () => new DataView(instance.exports.memory.buffer);
  const argc = instance.exports.malloc(4);
  view().setUint32(argc, args.length, true);
  const argv = instance.exports.malloc(4 * (args.length + 1));
  args.forEach((a, i) => {
    const b = enc.encode(a);
    const p = instance.exports.malloc(b.length + 1);
    new Uint8Array(instance.exports.memory.buffer, p, b.length).set(b);
    view().setUint8(p + b.length, 0);
    view().setUint32(argv + 4 * i, p, true);
  });
  view().setUint32(argv + 4 * args.length, 0, true);
  const argvp = instance.exports.malloc(4);
  view().setUint32(argvp, argv, true);
  instance.exports.hs_init_with_rtsopts(argc, argvp);
  return { instance, root };
}

function put(root, path, data, readonly) {
  const parts = path.split("/").filter((p) => p && p !== ".");
  let dir = root;
  for (const p of parts.slice(0, -1)) {
    let d = dir.get(p);
    if (!(d instanceof Directory)) {
      d = new Directory(new Map());
      dir.set(p, d);
    }
    dir = d.contents;
  }
  dir.set(parts[parts.length - 1], new File(data, { readonly }));
}

function walk(map, prefix, out) {
  for (const [name, e] of map.entries()) {
    const path = prefix ? `${prefix}/${name}` : name;
    if (e instanceof Directory) walk(e.contents, path, out);
    else out.push([path, e.data]);
  }
}

function warningText(w) {
  if (typeof w === "string") return w;
  if (w && typeof w === "object") {
    // "pretty" matches the native binary's "[WARNING] …" line.
    if (w.pretty) return String(w.pretty);
    if (w.message) return String(w.message);
    if (w.type) return [w.type, ...Object.entries(w).filter(([k]) => k !== "type" && k !== "verbosity").map(([, v]) => typeof v === "string" ? v : JSON.stringify(v))].join(": ");
  }
  return JSON.stringify(w);
}

/** Runs one request frame; returns a result frame (never throws). */
export function runFrame(pandoc, frame) {
  try {
    const { head, body } = unpack("FOPQ1", frame);
    const { instance, root } = pandoc;
    root.clear();
    const std = {};
    for (const n of ["stdin", "stdout", "stderr", "warnings"]) {
      std[n] = new File(new Uint8Array(), { readonly: n === "stdin" });
      root.set(n, std[n]);
    }
    const inputs = new Set(Object.keys(std));
    let at = 0;
    for (const f of head.files) {
      put(root, f.name, body.slice(at, at + f.len), true);
      inputs.add(f.name.split("/").filter(Boolean).join("/"));
      at += f.len;
    }
    const opts = head.options || {};
    if (opts["output-file"]) put(root, opts["output-file"], new Uint8Array(), false);
    const json = enc.encode(JSON.stringify(opts));
    const p = instance.exports.malloc(json.length);
    new Uint8Array(instance.exports.memory.buffer, p, json.length).set(json);
    instance.exports.convert(p, json.length);

    const stderr = dec.decode(std.stderr.data);
    const stdout = std.stdout.data;
    let warnings = [];
    try {
      const raw = dec.decode(std.warnings.data);
      // pandoc logs INFO messages (e.g. LoadedResource) here too; keep warnings.
      if (raw) warnings = JSON.parse(raw).filter((w) => !w || typeof w !== "object" || !w.verbosity || w.verbosity === "WARNING" || w.verbosity === "ERROR").map(warningText);
    } catch (_) {}
    const files = [];
    walk(root, "", files);
    const produced = files.filter(([n, d]) => !inputs.has(n) && d && d.length > 0);
    const wanted = opts["output-file"];
    if (wanted && !produced.some(([n]) => n === wanted) && stderr.trim()) {
      return errorFrame(stderr.trim());
    }
    if (!wanted && stdout.length === 0 && stderr.trim()) {
      return errorFrame(stderr.trim());
    }
    const headOut = {
      stderr,
      warnings,
      stdout_len: stdout.length,
      files: produced.map(([name, d]) => ({ name, len: d.length })),
    };
    return pack("FOPR1", headOut, [stdout, ...produced.map(([, d]) => d)]);
  } catch (e) {
    return errorFrame(e && e.message ? e.message : String(e));
  }
}
