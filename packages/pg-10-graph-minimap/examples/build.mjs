// SPDX-License-Identifier: MIT OR Apache-2.0
import { build } from "esbuild";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
const root = fileURLToPath(new URL("../", import.meta.url));
const out = new URL("../dist/demo/", import.meta.url);
await mkdir(out, { recursive: true });
const result = await build({
  absWorkingDir: root,
  entryPoints: ["examples/demo.tsx"],
  bundle: true,
  write: false,
  format: "iife",
  platform: "browser",
  target: "es2022",
  jsx: "automatic",
  define: { "process.env.NODE_ENV": '"production"' },
});
const script = result.outputFiles[0].text.replace(/<\/script/gi, "<\\/script");
await writeFile(
  new URL("index.html", out),
  `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Synthetic graph minimap</title><style>body{font:16px system-ui;margin:24px;background:#fff;color:#172033}button{font:inherit;padding:4px 10px}h1{font-size:24px}pre{white-space:pre-wrap}canvas{display:block}</style><main id="root"></main><script>${script}</script></html>`,
);
console.log("Built self-contained offline dist/demo/index.html");
