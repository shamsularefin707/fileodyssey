// node test.mjs — drives the real pandoc.wasm through the worker's code path.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createPandoc, pack, runFrame, unpack } from "./pandoc-core.js";

const wasm = readFileSync(fileURLToPath(new URL("../assets/pandoc/pandoc.wasm", import.meta.url)));
const enc = new TextEncoder();
const dec = new TextDecoder();
let failed = 0;
const check = (ok, msg) => { if (!ok) { failed++; console.log("FAIL", msg); } else console.log("ok  ", msg); };

function request(options, files) {
  const blobs = files.map(([, d]) => (typeof d === "string" ? enc.encode(d) : d));
  return pack("FOPQ1", { options, files: files.map(([name], i) => ({ name, len: blobs[i].length })) }, blobs);
}
function result(frame) {
  const { head, body } = unpack("FOPR1", frame);
  if (head.error) return { error: head.error };
  let at = head.stdout_len;
  const files = {};
  for (const f of head.files) { files[f.name] = body.slice(at, at + f.len); at += f.len; }
  return { stdout: dec.decode(body.slice(0, head.stdout_len)), files, warnings: head.warnings, stderr: head.stderr };
}

const t0 = Date.now();
const pandoc = await createPandoc(wasm);
console.log("init ms", Date.now() - t0);
const png = readFileSync(fileURLToPath(new URL("../../../tests/corpus/pics/river.png", import.meta.url)));

let r = result(runFrame(pandoc, request(
  { from: "markdown", to: "docx", "input-files": ["input.md"], "output-file": "out.docx" },
  [["input.md", "# শিরোনাম\n\n![River](pics/river.png)\n\nText[^1].\n\n[^1]: Note.\n"], ["pics/river.png", png]],
)));
check(!r.error && r.files["out.docx"] && r.files["out.docx"][0] === 0x50, "md → docx with an image from a subfolder");
check((r.warnings || []).length === 0, "no 'could not fetch' warning: " + JSON.stringify(r.warnings));
const docx = r.files["out.docx"];

r = result(runFrame(pandoc, request(
  { from: "docx", to: "markdown", standalone: true, "input-files": ["input.docx"], "output-file": "out.md", "extract-media": "media" },
  [["input.docx", docx]],
)));
const md = r.files && r.files["out.md"] ? dec.decode(r.files["out.md"]) : "";
check(md.includes("শিরোনাম") && md.includes("media/"), "docx → md keeps Bengali and refers to media/");
check(Object.keys(r.files || {}).some((n) => n.startsWith("media/") && n.endsWith(".png")), "extracted image comes back: " + Object.keys(r.files || {}));

r = result(runFrame(pandoc, request({ from: "docx", to: "json", "input-files": ["input.docx"] }, [["input.docx", docx]])));
check(!r.error && JSON.parse(r.stdout).blocks.length > 0, "docx → json AST on stdout");

r = result(runFrame(pandoc, request({ from: "nonsenseformat", to: "html", "input-files": ["a.txt"] }, [["a.txt", "x"]])));
check(typeof r.error === "string" && r.error.toLowerCase().includes("nonsenseformat"), "pandoc error reported: " + JSON.stringify(r.error));

r = result(runFrame(pandoc, request({ from: "markdown", to: "html", "input-files": ["a.md"] }, [["a.md", "*after the error*"]])));
check(r.stdout && r.stdout.includes("<em>after the error</em>"), "instance still works after an error");

check(result(runFrame(pandoc, new Uint8Array([1, 2, 3]).buffer)).error, "damaged frame is an error, not a crash");
r = result(runFrame(pandoc, request({ from: "markdown", to: "docx", "input-files": ["a.md"], "output-file": "o.docx" }, [["a.md", "![x](missing.png)"]])));
check(r.warnings.length === 1 && r.warnings[0].startsWith("Could not fetch resource missing.png"), "warnings read like pandoc's own: " + JSON.stringify(r.warnings));
console.log(failed ? `${failed} failed` : "PASS");
process.exit(failed ? 1 : 0);
