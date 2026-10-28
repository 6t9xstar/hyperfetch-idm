import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(root, "extension");
const output = resolve(root, "extension", "build");
const staging = mkdtempSync(join(tmpdir(), "hyperfetch-extension-"));

rmSync(output, { recursive: true, force: true });

const targets = ["chrome", "firefox"];
for (const target of targets) {
  cpSync(source, join(staging, target), { recursive: true });

  const firefoxManifest = join(staging, target, "manifest.firefox.json");
  if (target === "firefox") {
    const manifest = readFileSync(firefoxManifest, "utf8");
    writeFileSync(join(staging, target, "manifest.json"), manifest);
  }
  rmSync(firefoxManifest, { force: true });
}

mkdirSync(output, { recursive: true });
for (const target of targets) {
  cpSync(join(staging, target), join(output, target), { recursive: true });
}
rmSync(staging, { recursive: true, force: true });

console.log(`Extension builds written to ${output}`);
