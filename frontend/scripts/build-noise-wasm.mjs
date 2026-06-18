import { existsSync, mkdirSync, copyFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(import.meta.dirname, "../..");
const output = resolve(repoRoot, "frontend/public/wasm/lyre_noise_wasm.wasm");

if (process.env.LYRE_USE_EXISTING_NOISE_WASM === "1") {
  if (existsSync(output)) {
    process.exit(0);
  }
  console.error(`missing prebuilt noise wasm at ${output}`);
  process.exit(1);
}

const build = spawnSync(
  "cargo",
  ["build", "--release", "-p", "lyre-noise-wasm", "--target", "wasm32-unknown-unknown"],
  {
    cwd: repoRoot,
    stdio: "inherit"
  }
);

if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

mkdirSync(dirname(output), { recursive: true });
copyFileSync(
  resolve(repoRoot, "target/wasm32-unknown-unknown/release/lyre_noise_wasm.wasm"),
  output
);
