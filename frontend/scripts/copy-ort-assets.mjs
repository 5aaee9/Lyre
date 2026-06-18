import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";

const frontendRoot = resolve(import.meta.dirname, "..");
const wasmName = "ort-wasm-simd-threaded.wasm";
const require = createRequire(import.meta.url);
const wasmSource = require.resolve(`onnxruntime-web/${wasmName}`);
const output = resolve(frontendRoot, "public/ort", wasmName);

mkdirSync(dirname(output), { recursive: true });
copyFileSync(wasmSource, output);
chmodSync(output, 0o644);
