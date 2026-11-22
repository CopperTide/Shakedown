// A run's world drawn from its world's view files: the view's screens as tabs, live or scrubbed.
import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import { StatusChip } from '@/components/marks';
import { cn } from '@/lib/cn';
import RunMenu from '@/features/run/RunMenu';
import { useStepParam } from '@/features/run/useStepParam';
import { moveStop, trackMarks } from '@/features/world/scrub';
import { usePlay, useStopKeys } from '@/features/world/useWorld';
import type { BranchDetail } from '@/model/types';
import { atFor, isLive, viewStops } from './points';
import type { ScreenDef, ViewDef, WorldViews } from './types';
import { useScreen } from './useViews';
import ViewScreen from './ViewScreen';
import ViewScrubber, { LiveClock, WorldTime } from './ViewScrubber';

function useParam(name: string): [string | null, (v: string) => void] {
  const [sp, setSp] = useSearchParams();
  const set = useCallback((v: string) => setSp((prev) => { const out = new URLSearchParams(prev); out.set(name, v); return out; }, { replace: true }), [name, setSp]);
  return [sp.get(name), set];
}

function ScreenTabs({ view, screen, onScreen }: { view: ViewDef; screen: ScreenDef | undefined; onScreen: (id: string) => void }) {
  if (view.screens.length < 2) return null;
  return (
    <nav aria-label="Screens" className="flex h-full items-stretch gap-0.5">
      {view.screens.map((s) => (
        <button key={s.id} type="button" aria-current={s.id === screen?.id ? 'page' : undefined} onClick={() => onScreen(s.id)}
          className={cn('-mb-px border-b-2 px-2.5 text-[13px] font-medium', s.id === screen?.id ? 'border-accent text-fg' : 'border-transparent text-muted hover:text-fg')}>
          {s.label}
        </button>
      ))}
    </nav>
  );
}

function ViewPicker({ views, view, onView }: { views: ViewDef[]; view: ViewDef; onView: (id: string) => void }) {
  if (views.length < 2) return null;
  return (
    <select aria-label="View" value={view.id} onChange={(e) => onView(e.target.value)}
      className="h-7 rounded-[3px] border border-line bg-raised px-1.5 text-xs text-fg">
      {views.map((v) => <option key={v.id} value={v.id}>{v.label}</option>)}
    </select>
  );
}

/** The recorded branch's stops, the one on screen, and the scrubber's controls (←/→, space). */
function useStops(d: BranchDetail) {
  const stops = useMemo(() => viewStops(d.events), [d.events]);
  const seqs = useMemo(() => stops.map((s) => s.seq), [stops]);
  const marks = useMemo(() => trackMarks(d.events, d.start_seq), [d.events, d.start_seq]);
  const [step, setStep] = useStepParam();
  const point = atFor(stops, step);
  const play = usePlay(seqs, step, setStep);
  const { pause } = play;
  const move = useCallback((dir: 'prev' | 'next') => { pause(); setStep((cur) => moveStop(seqs, cur, dir)); }, [pause, setStep, seqs]);
  useStopKeys(seqs, setStep, play.toggle, pause);
  return { stops, marks, point, play, move, setStep };
}

/** The view on screen and its screen, from the address (the first of each when it names none). */
function useChosen(defs: WorldViews) {
  const [viewId, setView] = useParam('view');
  const [screenId, setScreen] = useParam('screen');
  const view = defs.views.find((v) => v.id === viewId) ?? defs.views[0];
  const screen: ScreenDef | undefined = view.screens.find((s) => s.id === screenId) ?? view.screens[0];
  return { view, screen, setView, setScreen };
}

function Clock({ live, sd }: { live: boolean; sd: ReturnType<typeof useScreen> }) {
  if (live) return <LiveClock at={sd.data?.at ?? null} fetchedAt={sd.fetchedAt} />;
  return <WorldTime ms={sd.data?.at.world_ms ?? null} className={cn(sd.pending && 'opacity-60')} />;
}

/** The world's declarative views for a branch: header, screen tabs, the screen, and the live clock or the scrubber. */
export default function ViewsWorld({ runId, d, defs }: { runId: string; d: BranchDetail; defs: WorldViews }) {
  const { view, screen, setView, setScreen } = useChosen(defs);
  const live = isLive(d.status);
  const s = useStops(d);
  const sd = useScreen(d.id, view.id, screen?.id ?? null, s.point.at, live, d.events.length);
  return (
    <div className="flex flex-col md:h-[calc(100dvh-48px)] md:min-h-[560px]">
      <header className="flex min-h-11 shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-line px-4 py-1 sm:px-5">
        <h1 className="truncate font-display text-[24px] leading-none" title={defs.world ?? undefined}>{view.label}</h1>
        <StatusChip status={d.status} />
        <ViewPicker views={defs.views} view={view} onView={setView} />
        <ScreenTabs view={view} screen={screen} onScreen={setScreen} />
        <div className="ml-auto flex min-w-0 items-center gap-3">
          {sd.error && <span className="truncate text-xs text-tone-critical" title={sd.error}>✕ {sd.error}</span>}
          <Clock live={live} sd={sd} />
          <RunMenu runId={runId} d={d} page="world" step={s.point.seq} />
        </div>
      </header>
      {screen
        ? <ViewScreen view={view} screen={screen} data={sd.data} fetchedAt={sd.fetchedAt} />
        : <div className="flex-1 px-5 py-4 text-sm text-muted">This view has no screens.</div>}
      {!live && s.stops.length > 0 && (
        <ViewScrubber stops={s.stops} marks={s.marks} current={s.point.seq} playing={s.play.playing} pending={sd.pending}
          onPlay={s.play.toggle} onStep={(seq) => { s.play.pause(); s.setStep(seq); }} onMove={s.move} />
      )}
    </div>
  );
}
