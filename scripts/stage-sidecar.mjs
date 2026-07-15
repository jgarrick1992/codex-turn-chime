import { copyFile, mkdir, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import process from "node:process";

const [target, profile = "release"] = process.argv.slice(2);
if (!target) {
  console.error("Usage: node scripts/stage-sidecar.mjs <target-triple> [profile]");
  process.exit(2);
}

const windows = target.includes("windows");
const executable = `codex-turn-chime-hook${windows ? ".exe" : ""}`;
const source = resolve("src-tauri", "target", target, profile, executable);
const destination = resolve(
  "src-tauri",
  "binaries",
  `codex-turn-chime-hook-${target}${windows ? ".exe" : ""}`,
);

try {
  const info = await stat(source);
  if (!info.isFile()) throw new Error("source is not a file");
  await mkdir(dirname(destination), { recursive: true });
  await copyFile(source, destination);
  console.log(`Staged sidecar: ${join("src-tauri", "binaries", destination.split(/[\\/]/).at(-1))}`);
} catch (error) {
  console.error(`Unable to stage ${source}: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
}
