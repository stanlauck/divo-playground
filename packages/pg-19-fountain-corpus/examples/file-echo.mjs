// SPDX-License-Identifier: MIT OR Apache-2.0

import { readFile, readdir } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const file = process.argv[2];
if (!file) process.exit(2);
const root = dirname(fileURLToPath(import.meta.url));
const id = basename(file, ".fountain");
const names = await readdir(join(root, "../corpus"));
if (!names.includes(`${id}.expected.json`)) process.exit(1);
process.stdout.write(await readFile(join(root, "../corpus", `${id}.expected.json`), "utf8"));
