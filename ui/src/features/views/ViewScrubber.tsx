import { ChevronLeft, ChevronRight, Pause, Play } from 'lucide-react';
import { type PointerEvent, useRef } from 'react';
import { cn } from '@/lib/cn';
import { type TrackMark, nearestStop, trackAt } from '@/features/world/scrub';
import { dateIn, timeIn } from './format';
import { RATES } from './playback';
import type { Stop } from './points';
import type { At } from './types';
import { useWorldNow } from './useWorldNow';
import { useZone } from './zone';

const MARK: Record<TrackMark['kind'], { glyph: string; cls: string }> = {
  disruption: { glyph: '▼', cls: 'text-tone-warn' },
  refusal: { glyph: '▲', cls: 'text-tone-critical' },
  fork: { glyph: '┃', cls: 'text-faint' },
};

/** The world time of the point on screen, in the world's zone: "10:05", its date and the zone ("UTC−4"; "14:05Z" in UTC). */
export function WorldTime({ ms, seconds = false, className }: { ms: number | null; seconds?: boolean; className?: string }) {
  const zone = useZone();
  if (ms == null) return <span className={cn('font-mono text-xs text-faint', className)}>No world clock</span>;
  return (
    <span className={cn('flex items-baseline gap-2 whitespace-nowrap', className)} title={new Date(ms).toISOString()}>
      <span className="font-mono text-[15px] font-medium tabular-nums">{timeIn(ms, zone, seconds)}</span>
      {zone.label !== 'Z' && <span className="font-mono text-[11px] text-muted" title="World local time">{zone.label}</span>}
      <span className="text-[11px] text-muted tabular-nums">{dateIn(ms, zone)}</span>
    </span>
  );
}

/** "Live 2,880×" (world time per real time), "Playing" (its rate is on the scrubber) or "Paused": the running clock's state. */
function ClockState({ at, label }: { at: At | null; label: string }) {
  if (at?.paused) return <span className="flex items-center gap-1.5 text-xs"><span aria-hidden="true" className="font-mono">‖</span>Paused</span>;
  const speed = label === 'Live' && at && at.speed > 0 ? `${Math.round(at.speed).toLocaleString('en-US')}×` : null;
  return (
    <span className="flex items-center gap-1.5 text-xs">
      <span aria-hidden="true" className="size-1.5 animate-pulse rounded-full bg-copper" />{label}
      {speed && <span className="font-mono text-[11px] text-faint tabular-nums" title="World time per real time">{speed}</span>}
    </span>
  );
}

/** A running world clock (live, or a recording playing), run on between answers at the clock's speed. */
export function LiveClock({ at, fetchedAt, label = 'Live' }: { at: At | null; fetchedAt: number; label?: string }) {
  return (
    <span className="flex items-center gap-3">
      <ClockState at={at} label={label} />
      <WorldTime ms={useWorldNow(at, fetchedAt)} seconds />
    </span>
  );
}

function Track({ stops, marks, current, onStep }: { stops: Stop[]; marks: TrackMark[]; current: number | null; onStep: (seq: number) => void }) {
  const box = useRef<HTMLDivElement>(null);
  const seqs = stops.map((s) => s.seq);
  const t = { min: seqs[0] ?? 0, max: seqs[seqs.length - 1] ?? 0 };
  const pct = (seq: number) => `${trackAt(seq, t) * 100}%`;
  const pick = (e: PointerEvent<HTMLDivElement>) => {
    const r = box.current?.getBoundingClientRect();
    if (!r || r.width === 0) return;
    const s = nearestStop(seqs, (e.clientX - r.left) / r.width, t);
    if (s != null) onStep(s);
  };
  return (
    <div ref={box} className="relative h-7 min-w-24 flex-1 cursor-pointer touch-none select-none"
      onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); pick(e); }} onPointerMove={(e) => { if (e.buttons === 1) pick(e); }}>
      <div className="absolute inset-x-0 top-[17px] h-px bg-line-strong" />
      {current != null && <div className="absolute left-0 top-[16px] h-[3px] bg-accent" style={{ width: pct(current) }} />}
      {stops.map((s) => <div key={s.seq} className={cn('absolute top-[13px] h-[9px] w-px', s.at === 'final' ? 'bg-fg' : 'bg-line-strong')} style={{ left: pct(s.seq) }} />)}
      {marks.map((m) => <span key={`${m.kind}-${m.seq}`} title={m.label} className={cn('absolute top-0 -translate-x-1/2 font-mono text-[10px] leading-3', MARK[m.kind].cls)} style={{ left: pct(m.seq) }}>{MARK[m.kind].glyph}</span>)}
      {current != null && <div className="absolute top-[11px] h-[13px] w-[5px] -translate-x-1/2 rounded-[1px] border border-surface bg-accent" style={{ left: pct(current) }} />}
    </div>
  );
}

export interface ScrubProps {
  stops: Stop[];
  marks: TrackMark[];
  current: number | null;
  playing: boolean;
  pending: boolean;
  /** World ms per real ms that Play runs at, one of `RATES`. */
  rate: number;
  onRate: (rate: number) => void;
  onPlay: () => void;
  onStep: (seq: number) => void;
  onMove: (dir: 'prev' | 'next') => void;
}

const BTN = 'flex size-7 shrink-0 items-center justify-center rounded-[3px] border border-line bg-raised text-muted hover:border-line-strong hover:text-fg [&_svg]:size-3.5';

/** Play's rate: 1× (an hour of world time in 10 s), 4×, 16×. */
function Rates({ rate, onRate }: { rate: number; onRate: (rate: number) => void }) {
  return (
    <div className="flex items-center rounded-[3px] border border-line" role="group" aria-label="Playback rate">
      {RATES.map((r) => (
        <button key={r.label} type="button" aria-pressed={r.worldMsPerMs === rate} onClick={() => onRate(r.worldMsPerMs)}
          title={`${r.label}: ${Math.round(r.worldMsPerMs / 360) === 1 ? 'an hour' : `${r.worldMsPerMs / 360} hours`} of world time in 10 s`}
          className={cn('h-[26px] px-1.5 font-mono text-[11px] tabular-nums', r.worldMsPerMs === rate ? 'bg-raised text-fg' : 'text-faint hover:text-fg')}>
          {r.label}
        </button>
      ))}
    </div>
  );
}

/** The track over a recorded branch's points: play at a rate, step, click or drag; which step is on screen. */
export default function ViewScrubber(p: ScrubProps) {
  const stop = p.stops.find((s) => s.seq === p.current);
  const index = stop ? p.stops.indexOf(stop) : -1;
  return (
    <div className="flex h-10 items-center gap-3 border-t border-line bg-surface px-3">
      <div className="flex items-center gap-1">
        <button type="button" className={cn(BTN, 'border-fg bg-fg text-bg hover:text-bg')} onClick={p.onPlay} aria-label={p.playing ? 'Pause' : 'Play'} title={p.playing ? 'Pause (space)' : 'Play the day (space)'}>{p.playing ? <Pause /> : <Play />}</button>
        <button type="button" className={BTN} onClick={() => p.onMove('prev')} aria-label="Previous step" title="Previous step (←)"><ChevronLeft /></button>
        <button type="button" className={BTN} onClick={() => p.onMove('next')} aria-label="Next step" title="Next step (→)"><ChevronRight /></button>
        <Rates rate={p.rate} onRate={p.onRate} />
      </div>
      <Track stops={p.stops} marks={p.marks} current={p.current} onStep={p.onStep} />
      <span className={cn('w-[124px] shrink-0 text-right font-mono text-[11px] text-muted tabular-nums', p.pending && 'text-faint')} title={`Point ${index + 1} of ${p.stops.length}`}>
        {stop ? (stop.at === 'final' ? 'Final state' : `Step ${stop.seq}`) : 'No steps'}
        <span className="text-faint"> · {index + 1}/{p.stops.length}</span>
      </span>
    </div>
  );
}
