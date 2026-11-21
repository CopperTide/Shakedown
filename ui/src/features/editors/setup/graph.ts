// Lays out a team graph in columns: agents nothing points to first, then each agent one column
// after the furthest agent that points to it. Pure.
import type { MadlGraph } from '@/api/project';

export interface PlacedNode { id: string; label: string; runtime?: string | null; column: number; row: number }

export function layout(graph: MadlGraph): { nodes: PlacedNode[]; columns: number } {
  const column = new Map(graph.agents.map((a) => [a.id, 0]));
  for (let pass = 0; pass < graph.agents.length; pass++) {
    for (const e of graph.edges) {
      const from = column.get(e.from);
      if (from != null && column.has(e.to) && (column.get(e.to) ?? 0) < from + 1 && e.from !== e.to) column.set(e.to, from + 1);
    }
  }
  const rows = new Map<number, number>();
  const nodes = graph.agents.map((a) => {
    const c = Math.min(column.get(a.id) ?? 0, graph.agents.length - 1);
    const row = rows.get(c) ?? 0;
    rows.set(c, row + 1);
    return { ...a, column: c, row };
  });
  return { nodes, columns: Math.max(1, ...nodes.map((n) => n.column + 1)) };
}
