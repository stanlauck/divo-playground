// SPDX-License-Identifier: MIT OR Apache-2.0

import { open } from "node:fs/promises";
import {
  MAX_CORPUS_JSON_UTF16,
  UnicodeError,
  checkCorpus,
  parseCorpus,
} from "../src/index.js";

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  if (args.length !== 1) {
    throw new UnicodeError("invalid_corpus", "Usage: check <corpus.json>");
  }
  const handle = await open(args[0]!, "r");
  try {
    const maxBytes = MAX_CORPUS_JSON_UTF16 * 3;
    const stat = await handle.stat();
    if (!stat.isFile() || stat.size > maxBytes) {
      throw new UnicodeError(
        "limit_exceeded",
        "Corpus file is not a bounded regular file",
      );
    }
    const bytes = Buffer.alloc(stat.size + 1);
    let length = 0;
    while (length < bytes.length) {
      const read = await handle.read(
        bytes,
        length,
        bytes.length - length,
        null,
      );
      if (read.bytesRead === 0) break;
      length += read.bytesRead;
    }
    if (length > stat.size) {
      throw new UnicodeError(
        "limit_exceeded",
        "Corpus file grew beyond its initial size",
      );
    }
    const json = new TextDecoder("utf-8", { fatal: true }).decode(
      bytes.subarray(0, length),
    );
    const report = checkCorpus(parseCorpus(json));
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    if (report.failed !== 0) {
      process.exitCode = 1;
    }
  } finally {
    await handle.close();
  }
}

main().catch((error: unknown) => {
  const message =
    error instanceof UnicodeError
      ? `${error.code}: ${error.message}`
      : "Unable to read UTF-8 corpus";
  process.stderr.write(`${message}\n`);
  process.exitCode = 1;
});
