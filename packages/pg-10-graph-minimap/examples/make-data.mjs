// SPDX-License-Identifier: MIT OR Apache-2.0
import { writeFile } from "node:fs/promises";
import { makeDocument } from "./fixture.mjs";
export function makeSchema() {
  const number = { type: "number", minimum: -1e9, maximum: 1e9 };
  const extent = { type: "number", minimum: 1e-6, maximum: 2e9 };
  const id = {
    type: "string",
    minLength: 1,
    maxLength: 128,
    pattern:
      "^[^\\u0000-\\u0020\\u007f-\\u009f](?:[^\\u0000-\\u001f\\u007f-\\u009f]*[^\\u0000-\\u0020\\u007f-\\u009f])?$",
  };
  const object = (properties, required = Object.keys(properties)) => ({
    type: "object",
    required,
    properties,
    additionalProperties: false,
  });
  const rect = { x: number, y: number, width: extent, height: extent };
  return {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    title: "PG-10 neutral minimap document v1",
    description:
      "Shape only. Runtime checks UTF-8 ID byte limits, references, uniqueness, finite geometry and budgets. Polyline points describe a complete world-space route, including endpoints.",
    ...object({
      version: { const: 1, type: "integer" },
      nodes: {
        type: "array",
        maxItems: 100_000,
        items: object({ id, ...rect }),
      },
      edges: {
        type: "array",
        maxItems: 200_000,
        items: object(
          {
            id,
            source: id,
            target: id,
            points: {
              type: "array",
              minItems: 2,
              maxItems: 10_000,
              items: object({ x: number, y: number }),
            },
          },
          ["id", "source", "target"],
        ),
      },
      viewport: object(rect),
    }),
  };
}
if (process.argv[1]?.endsWith("make-data.mjs")) {
  if (process.argv[2] !== "--write")
    throw new Error("Usage: node examples/make-data.mjs --write");
  await writeFile(
    new URL("../schema.json", import.meta.url),
    JSON.stringify(makeSchema(), null, 2) + "\n",
  );
  await writeFile(
    new URL("../samples/synthetic.json", import.meta.url),
    JSON.stringify(makeDocument(), null, 2) + "\n",
  );
}
