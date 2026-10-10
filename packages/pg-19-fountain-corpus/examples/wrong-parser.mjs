// SPDX-License-Identifier: MIT OR Apache-2.0

process.stdin.resume();
process.stdin.on("end", () => process.stdout.write(JSON.stringify({ version: 1, title_page: [], elements: [{ type: "action", text: "deliberately wrong" }] })));
