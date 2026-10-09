// SPDX-License-Identifier: MIT OR Apache-2.0
import { build } from "esbuild";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
const root = fileURLToPath(new URL("../", import.meta.url));
const result = await build({
  absWorkingDir: root,
  entryPoints: ["examples/demo.ts"],
  bundle: true,
  format: "iife",
  platform: "browser",
  target: "es2022",
  minify: true,
  write: false,
});
const script = result.outputFiles[0].text.replace(/<\/script/gi, "<\\/script");
await mkdir(new URL("../dist/demo/", import.meta.url), { recursive: true });
await writeFile(
  new URL("../dist/demo/index.html", import.meta.url),
  `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>PG-11 synthetic themes</title>
<style>body{font-family:system-ui;margin:24px;background:var(--pg11-background);color:var(--pg11-text);font-size:var(--pg11-font-size);line-height:var(--pg11-line-height)}label{display:inline-block;margin:8px}.card,.raised{padding:var(--pg11-padding);margin:var(--pg11-gap) 0;border:1px solid var(--pg11-border);border-radius:var(--pg11-radius)}.card{background:var(--pg11-surface)}.raised{background:var(--pg11-raised)}.muted{color:var(--pg11-muted-text)}a{color:var(--pg11-accent)}button,input,select{min-height:var(--pg11-control-height);font:inherit}button.accent{background:var(--pg11-accent);color:var(--pg11-on-accent);border:0;padding:0 var(--pg11-padding)}:focus-visible{outline:2px solid var(--pg11-focus);outline-offset:2px}pre{white-space:pre-wrap}</style><main id="root"></main><script>${script}</script></html>`,
);
console.log("Built offline dist/demo/index.html");
