// An Explore chart in SVG, like the Overview's usage bars: categories along x, bars (side by side,
// or stacked) or lines per series, exact values on hover, and each bar or point a link to the rows
// behind it. The table below always holds every value.
import { useEffect, useRef, useState } from 'react';
import { Legend } from '../parts';
import { colorOf } from '../colors';
import { type ChartData, stackBases, topOf } from './chartData';
import { axisText, measureText } from './values';

export type Kind = 'bar' | 'stacked' | 'line';
const H = 240, TOP = 12, BOTTOM = 44, LEFT = 56, RIGHT = 12;
const PLOT_H = H - TOP - BOTTOM;

/** The box's width in CSS pixels, so labels keep their size on a narrow card. */
function useWidth() {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(640);
  useEffect(() => {
    const el = box.current;
    if (!el) return undefined;
    const measure = () => { if (el.clientWidth > 0) setWidth(Math.max(280, el.clientWidth)); };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return { box, width };
}

interface Geo { band: number; y: (v: number) => number; x: (i: number) => number }
type Pick = (xi: number, si: number) => void;

const clip = (s: string, n: number) => (s.length > n ? `${s.slice(0, Math.max(1, n - 1))}…` : s);

function Bars({ d, geo, kind, colors, onPick }: { d: ChartData; geo: Geo; kind: Kind; colors: string[]; onPick?: Pick }) {
  const stacked = kind === 'stacked' && d.series.length > 1;
  const bases = stackBases(d);
  const n = stacked ? 1 : d.series.length;
  const w = Math.max(2, Math.min(48, (geo.band * 0.75) / n));
  return <>{d.series.map((s, si) => s.values.map((v, xi) => {
    if (v == null) return null;
    const base = stacked ? bases[si][xi] : 0;
    const left = geo.x(xi) - (w * n) / 2 + (stacked ? 0 : si * w);
    const top = geo.y(base + Math.max(0, v));
    return (
      <rect key={`${si}:${xi}`} x={left} y={top} width={w} height={Math.max(0, geo.y(base) - top)} fill={colors[si]}
        className={onPick ? 'cursor-pointer hover:opacity-80' : undefined} onClick={() => onPick?.(xi, si)}>
        <title>{`${d.labels[xi]}${d.series.length > 1 ? ` · ${s.name}` : ''}: ${measureText(v, d.unit)}`}</title>
      </rect>
    );
  }))}</>;
}

function Lines({ d, geo, colors, onPick }: { d: ChartData; geo: Geo; colors: string[]; onPick?: Pick }) {
  return <>{d.series.map((s, si) => {
    const pts = s.values.map((v, xi) => (v == null ? null : [geo.x(xi), geo.y(v)] as const));
    const line = pts.filter((p) => p != null).map((p) => p.join(',')).join(' ');
    return (
      <g key={si}>
        <polyline points={line} fill="none" stroke={colors[si]} strokeWidth={2} />
        {pts.map((p, xi) => p && (
          <circle key={xi} cx={p[0]} cy={p[1]} r={4} fill={colors[si]} stroke="var(--surface)" strokeWidth={1.5}
            className={onPick ? 'cursor-pointer' : undefined} onClick={() => onPick?.(xi, si)}>
            <title>{`${d.labels[xi]}${d.series.length > 1 ? ` · ${s.name}` : ''}: ${measureText(s.values[xi], d.unit)}`}</title>
          </circle>
        ))}
      </g>
    );
  })}</>;
}

function Axes({ d, geo, width, top }: { d: ChartData; geo: Geo; width: number; top: number }) {
  const every = Math.max(1, Math.ceil(d.labels.length / Math.max(1, Math.floor((width - LEFT) / 70))));
  const chars = Math.max(4, Math.floor((geo.band * every) / 6.5));
  return <>
    {[0, top / 2, top].map((v) => (
      <g key={v}>
        <line x1={LEFT} x2={width - RIGHT} y1={geo.y(v)} y2={geo.y(v)} stroke="var(--line)" />
        <text x={LEFT - 8} y={geo.y(v) + 4} textAnchor="end" fill="var(--muted)" fontSize="11">{axisText(v, d.unit)}</text>
      </g>
    ))}
    {d.labels.map((l, i) => i % every === 0 && (
      <text key={i} x={geo.x(i)} y={H - BOTTOM + 16} textAnchor="middle" fill="var(--muted)" fontSize="11">{clip(l, chars)}<title>{l}</title></text>
    ))}
  </>;
}

/** The chart, its legend when there are several series, and what it leaves out. */
export default function ExploreChart({ data, kind, label, onPick }: { data: ChartData; kind: Kind; label: string; onPick?: Pick }) {
  const { box, width } = useWidth();
  const top = topOf(data, kind === 'stacked');
  const band = (width - LEFT - RIGHT) / Math.max(1, data.labels.length);
  const geo: Geo = { band, x: (i) => LEFT + band * (i + 0.5), y: (v) => TOP + PLOT_H - (v / top) * PLOT_H };
  const names = data.series.map((s) => s.name);
  const colors = data.series.length === 1 ? ['var(--accent)'] : names.map((n) => colorOf(n, names));
  return (
    <div className="flex flex-col gap-2">
      <div ref={box} className="w-full">
        <svg viewBox={`0 0 ${width} ${H}`} width={width} height={H} role="img" aria-label={label} className="block">
          <Axes d={data} geo={geo} width={width} top={top} />
          {kind === 'line' ? <Lines d={data} geo={geo} colors={colors} onPick={onPick} /> : <Bars d={data} geo={geo} kind={kind} colors={colors} onPick={onPick} />}
        </svg>
      </div>
      {data.series.length > 1 && <Legend items={names.map((name, i) => ({ name, color: colors[i] }))} />}
      {(data.hidden.x > 0 || data.hidden.series > 0) && (
        <p className="text-xs text-faint">
          The chart shows the first {data.labels.length} of {data.labels.length + data.hidden.x}
          {data.hidden.series > 0 && ` and the ${data.series.length} biggest series (${data.hidden.series} more)`}; the table has every row.
        </p>
      )}
    </div>
  );
}
