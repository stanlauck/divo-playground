// SPDX-License-Identifier: MIT OR Apache-2.0

import { writeFile } from "node:fs/promises";
import { makeCorpus } from "./support/fixtures.mjs";

async function main() {
  const args = process.argv.slice(2);
  if (args.length !== 1) {
    throw new Error("Usage: make-corpus.mjs <new-corpus.json>");
  }
  // Generation uses the independently curated definitions, not the TS API.
  await writeFile(args[0], `${JSON.stringify(makeCorpus(), null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
}

main().catch(() => {
  process.stderr.write(
    "Unable to create a new corpus file; existing files are not overwritten\n",
  );
  process.exitCode = 1;
});
