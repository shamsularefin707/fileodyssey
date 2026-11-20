// Web Worker entry: { id, frame } in, { id, frame } out (frame transferred).
// The wasm module is fetched once from next to this script; if pandoc
// throws, the instance is dropped and a fresh one serves the next request.
import { createPandoc, errorFrame, runFrame } from "./pandoc-core.js";

let pandoc = null;

async function ready() {
  if (!pandoc) {
    const url = new URL("pandoc.wasm", self.location.href);
    const bytes = await (await fetch(url)).arrayBuffer();
    pandoc = await createPandoc(bytes);
  }
  return pandoc;
}

self.onmessage = async (e) => {
  const { id, frame } = e.data;
  let out;
  try {
    out = runFrame(await ready(), frame);
  } catch (err) {
    pandoc = null;
    out = errorFrame(err && err.message ? err.message : String(err));
  }
  self.postMessage({ id, frame: out }, [out]);
};
