import { Minus, Plus } from 'lucide-react';
import { memo, useEffect, useMemo, useState } from 'react';
import { cn } from '@/lib/cn';
import { type Zone, timeIn } from './format';
import { PanelEmpty, PanelFrame } from './PanelFrame';
import { type Bar, barsByLane, rangeOf, ticks, xOf, zoomStep } from './timeline';
import { type Tone, type TimelineData, type TimelineDef } from './types';
import { useScroll, useSize } from './useSize';
import { windowOf } from './virtual';
import { useZone } from './zone';

const LANE = 17;
const AXIS = 20;
const LABEL = 78;

const BAR: Record<Tone, string> = {
  neutral: 'bg-tone-neutral/22 text-muted', active: 'bg-tone-active text-surface', done: 'bg-tone-done/70 text-surface',
  warn: 'bg-tone-warn text-surface', critical: 'bg-tone-critical text-surface', accent: 'bg-tone-accent text-surface',
};

/** Bars whose chip is written after their label: the ones that need attention. */
const NAMED = new Set<Tone>(['warn', 'critical', 'accent']);

function BarMark({ bar, t0, t1, width, zone }: { bar: Bar; t0: number; t1: number; width: number; zone: Zone }) {
  const x = xOf(bar.start, t0, t1, width), w = Math.max(2, xOf(bar.end, t0, t1, width) - x);
  const tip = [bar.label || bar.key, `${timeIn(bar.start, zone)} – ${timeIn(bar.end, zone)}`, bar.chip].filter(Boolean).join(' · ');
  const text = bar.chip && NAMED.has(bar.tone) ? `${bar.label} ${bar.chip}` : bar.label;
  return (
    <div title={tip} className={cn('absolute top-[3px] h-[11px] overflow-hidden rounded-[1px] px-[3px] font-mono text-[9.5px] leading-[11px] whitespace-nowrap', BAR[bar.tone])}
      style={{ left: x, width: w }}>
      {w > text.length * 5.9 + 6 ? text : w > bar.label.length * 5.9 + 6 ? bar.label : ''}
    </div>
  );
}

/** A tick's label shows when it isn't cut by the axis's ends or under the now label. */
const labelFits = (x: number, width: number, nowX: number | null) => x > 18 && x < width - 18 && (nowX == null || Math.abs(x - nowX) > 40);

function Axis({ t0, t1, width, now, zone }: { t0: number; t1: number; width: number; now: number | null; zone: Zone }) {
  const ts = ticks(t0, t1, width, 64, zone);
  return (
    <div className="sticky top-0 z-20 flex border-b border-line-strong bg-surface" style={{ height: AXIS }}>
      <div className="sticky left-0 z-10 shrink-0 bg-surface" style={{ width: LABEL }} />
      <div className="relative shrink-0" style={{ width }}>
        {ts.filter((t) => labelFits(xOf(t.ms, t0, t1, width), width, now == null ? null : xOf(now, t0, t1, width))).map((t) => (
          <span key={t.ms} className={cn('absolute top-[3px] -translate-x-1/2 font-mono text-[10px] leading-3 tabular-nums', t.major ? 'text-muted' : 'text-faint')} style={{ left: xOf(t.ms, t0, t1, width) }}>{t.label}</span>
        ))}
        {now != null && now >= t0 && now <= t1 && (
          <span className="absolute top-[2px] -translate-x-1/2 rounded-[2px] bg-accent px-1 font-mono text-[10px] leading-[14px] text-accent-fg tabular-nums" style={{ left: xOf(now, t0, t1, width) }}>{timeIn(now, zone)}</span>
        )}
      </div>
    </div>
  );
}

function Grid({ t0, t1, width, height, now, zone }: { t0: number; t1: number; width: number; height: number; now: number | null; zone: Zone }) {
  return (
    <div className="pointer-events-none absolute" style={{ left: LABEL, top: AXIS, width, height }}>
      {ticks(t0, t1, width, 64, zone).map((t) => <div key={t.ms} className={cn('absolute inset-y-0 w-px', t.major ? 'bg-line' : 'bg-line/50')} style={{ left: xOf(t.ms, t0, t1, width) }} />)}
      {now != null && now >= t0 && now <= t1 && <div className="absolute inset-y-0 z-10 w-px bg-accent" style={{ left: xOf(now, t0, t1, width) }} />}
    </div>
  );
}

function Tools({ zoom, setZoom }: { zoom: number; setZoom: (z: number) => void }) {
  const btn = 'flex size-6 items-center justify-center rounded-[3px] border border-line text-muted hover:border-line-strong hover:text-fg disabled:opacity-40 [&_svg]:size-3';
  return (
    <div className="flex items-center gap-1">
      <button type="button" className={btn} aria-label="Zoom out" title="Zoom out (Ctrl + wheel)" disabled={zoom <= 1} onClick={() => setZoom(zoomStep(zoom, -1))}><Minus /></button>
      <button type="button" className={cn(btn, 'w-auto px-1.5 font-mono text-[10.5px] tabular-nums')} title="Fit the whole range" onClick={() => setZoom(1)}>{zoom === 1 ? 'Fit' : `${zoom}×`}</button>
      <button type="button" className={btn} aria-label="Zoom in" title="Zoom in (Ctrl + wheel)" onClick={() => setZoom(zoomStep(zoom, 1))}><Plus /></button>
    </div>
  );
}

/** One lane: its name, sticky at the left, and its bars; the same lane isn't drawn again as the timeline scrolls. */
const Lane = memo(function Lane({ index, name, bars, t0, t1, width, zone }: { index: number; name: string; bars: Bar[] | undefined; t0: number; t1: number; width: number; zone: Zone }) {
  return (
    <div className="absolute left-0 flex border-b border-line/60" style={{ top: AXIS + index * LANE, height: LANE, width: LABEL + width }}>
      <div className="sticky left-0 z-10 shrink-0 truncate border-r border-line bg-surface pr-1.5 pl-3 font-mono text-[10.5px] leading-[16px] text-muted tabular-nums" style={{ width: LABEL }} title={name}>{name}</div>
      <div className="relative shrink-0" style={{ width }}>
        {bars?.map((bar, j) => <BarMark key={`${bar.key}-${j}`} bar={bar} t0={t0} t1={t1} width={width} zone={zone} />)}
      </div>
    </div>
  );
});

/** Ctrl/⌘ + wheel zooms the time axis (a native listener: React's wheel events can't stop the page zooming). */
function useCtrlWheel(el: HTMLElement | null, setZoom: (f: (z: number) => number) => void) {
  useEffect(() => {
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      setZoom((z) => zoomStep(z, e.deltaY < 0 ? 1 : -1));
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [el, setZoom]);
}

/** One lane per entry of `lanes`, a bar per row, an axis in the world's zone and a now line; only the lanes in view are drawn. */
export default function TimelinePanel({ id, def, data, now }: { id: string; def: TimelineDef; data: TimelineData; now: number | null }) {
  const zone = useZone();
  const lanes = useMemo(() => barsByLane(data), [data]);
  const [t0, t1] = useMemo(() => rangeOf(data, lanes, now), [data, lanes, now]);
  const [zoom, setZoom] = useState(1);
  const size = useSize<HTMLDivElement>();
  const scroll = useScroll();
  const width = Math.max(200, (size.width - LABEL - 2) * zoom);
  const win = windowOf(scroll.top - AXIS, size.height || 600, LANE, data.lanes.length, 12);
  const height = data.lanes.length * LANE;
  useCtrlWheel(size.el, setZoom);
  return (
    <PanelFrame id={id} title={def.title} meta={`${data.lanes.length.toLocaleString('en-US')} lanes`} tools={<Tools zoom={zoom} setZoom={setZoom} />}>
      <div ref={size.ref} onScroll={scroll.onScroll} className="absolute inset-0 overflow-auto overscroll-contain">
        <div className="relative" style={{ width: LABEL + width, height: AXIS + height }}>
          <Axis t0={t0} t1={t1} width={width} now={now} zone={zone} />
          <Grid t0={t0} t1={t1} width={width} height={height} now={now} zone={zone} />
          {data.lanes.slice(win.start, win.end).map((name, k) => (
            <Lane key={win.start + k} index={win.start + k} name={name} bars={lanes[win.start + k]} t0={t0} t1={t1} width={width} zone={zone} />
          ))}
        </div>
        {!data.lanes.length && <PanelEmpty>No lanes.</PanelEmpty>}
      </div>
    </PanelFrame>
  );
}
