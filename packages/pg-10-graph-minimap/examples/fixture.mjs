// SPDX-License-Identifier: MIT OR Apache-2.0
export function makeGraph(count = 100) {
  const columns = Math.ceil(Math.sqrt(count));
  const nodes = Array.from({ length: count }, (_, i) => ({
    id: `node-${i}`,
    x: (i % columns) * 32 - 100,
    y: Math.floor(i / columns) * 24 - 60,
    width: 20,
    height: 12,
  }));
  const edges = [];
  for (let i = 0; i < count; i++) {
    if (i % columns !== columns - 1 && i + 1 < count)
      edges.push({
        id: `right-${i}`,
        source: `node-${i}`,
        target: `node-${i + 1}`,
      });
    if (i + columns < count)
      edges.push({
        id: `down-${i}`,
        source: `node-${i}`,
        target: `node-${i + columns}`,
      });
  }
  if (count > 1) {
    const a = nodes[0],
      b = nodes[count - 1];
    edges.push({
      id: "synthetic-route",
      source: a.id,
      target: b.id,
      points: [
        { x: a.x + 10, y: a.y + 6 },
        { x: a.x + 10, y: b.y + 6 },
        { x: b.x + 10, y: b.y + 6 },
      ],
    });
  }
  return { version: 1, nodes, edges };
}
export function makeDocument(count = 16) {
  return {
    ...makeGraph(count),
    viewport: { x: -80, y: -48, width: 80, height: 50 },
  };
}
