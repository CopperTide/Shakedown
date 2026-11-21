// The team drawn as boxes in columns, with an arrow for each connection.
import type { MadlGraph } from '@/api/project';
import { layout } from './graph';

const W = 168;
const H = 52;
const GAP_X = 72;
const GAP_Y = 24;

export default function TeamGraph({ graph }: { graph: MadlGraph }) {
  if (graph.agents.length === 0) return <p className="px-[18px] py-4 text-sm text-faint">No agents to draw.</p>;
  const { nodes, columns } = layout(graph);
  const at = new Map(nodes.map((n) => [n.id, { x: 16 + n.column * (W + GAP_X), y: 16 + n.row * (H + GAP_Y) }]));
  const height = 32 + Math.max(...nodes.map((n) => n.row + 1)) * (H + GAP_Y);
  return (
    <svg role="img" aria-label="Team graph" viewBox={`0 0 ${32 + columns * (W + GAP_X)} ${height}`} className="w-full">
      {graph.edges.map((e, i) => {
        const a = at.get(e.from);
        const b = at.get(e.to);
        if (!a || !b) return null;
        return <line key={i} x1={a.x + W} y1={a.y + H / 2} x2={b.x} y2={b.y + H / 2} stroke="var(--flow-strong)" strokeWidth={1.5} />;
      })}
      {nodes.map((n) => {
        const p = at.get(n.id)!;
        return (
          <g key={n.id}>
            <rect x={p.x} y={p.y} width={W} height={H} rx={8} fill="var(--surface)" stroke="var(--line-strong)" />
            <text x={p.x + 12} y={p.y + 22} fontSize={13} fill="var(--fg)">{n.label}</text>
            {n.runtime && <text x={p.x + 12} y={p.y + 40} fontSize={11} fill="var(--faint)">{n.runtime}</text>}
          </g>
        );
      })}
    </svg>
  );
}
