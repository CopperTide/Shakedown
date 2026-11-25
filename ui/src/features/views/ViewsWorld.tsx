// A run's world drawn from its world's view files: the view's screens as tabs, live, scrubbed or played back, every
// time in the world's own zone.
import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import { StatusChip } from '@/components/marks';
import { cn } from '@/lib/cn';
import RunMenu from '@/features/run/RunMenu';
import { useStepParam } from '@/features/run/useStepParam';
import { moveStop, trackMarks } from '@/features/world/scrub';
import { useStopKeys } from '@/features/world/useWorld';
import type { BranchDetail } from '@/model/types';
import { zoneOf } from './format';
import { atFor, isLive, viewStops } from './points';
import type { At, ScreenDef, ViewDef, WorldViews } from './types';
import { usePlayback } from './usePlayback';
import { useScreen } from './useViews';
import ViewScreen from './ViewScreen';
import ViewScrubber, { LiveClock, WorldTime } from './ViewScrubber';
import { ZoneContext } from './zone';

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

/** The recorded branch's stops, the one on screen, playback and the scrubber's controls (←/→, space). */
function useStops(d: BranchDetail, view: string, screen: string | null) {
  const stops = useMemo(() => viewStops(d.events), [d.events]);
  const seqs = useMemo(() => stops.map((s) => s.seq), [stops]);
  const marks = useMemo(() => trackMarks(d.events, d.start_seq), [d.events, d.start_seq]);
  const [step, setStep] = useStepParam();
  const point = atFor(stops, step);
  const shown = stops.findIndex((s) => s.seq === point.seq);
  const play = usePlayback({ branchId: d.id, view, screen, stops, shown, setStep });
  const { leave } = play;
  const move = useCallback((dir: 'prev' | 'next') => { leave(); setStep((cur) => moveStop(seqs, cur, dir)); }, [leave, setStep, seqs]);
  useStopKeys(seqs, setStep, play.toggle, leave);
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

function Clock({ live, playing, at, fetchedAt, pending }: { live: boolean; playing: boolean; at: At | null; fetchedAt: number; pending: boolean }) {
  if (live || playing) return <LiveClock at={at} fetchedAt={fetchedAt} label={live ? 'Live' : 'Playing'} />;
  return <WorldTime ms={at?.world_ms ?? null} className={cn(pending && 'opacity-60')} />;
}

/** "Shown at step 12996": the step asked for couldn't be restored, and why. */
function Fallback({ at }: { at: At | null }) {
  if (!at?.fallback) return null;
  return <span className="truncate text-xs text-tone-warn" title={at.fallback.why}>Shown at step {at.seq}</span>;
}

/** The screen to draw: its data with the `at` the clock shows (playback's virtual clock), and the world's zone. */
function useShown(live: boolean, sd: ReturnType<typeof useScreen>, play: ReturnType<typeof usePlayback>) {
  const at = sd.data?.at ?? null;
  const shown = live ? { at, fetchedAt: sd.fetchedAt } : play.shownAt(at, sd.fetchedAt);
  const data = sd.data && shown.at ? { ...sd.data, at: shown.at } : sd.data;
  return { ...shown, data, zone: zoneOf(at?.utc_offset_min) };
}

/** The world's declarative views for a branch: header, screen tabs, the screen, and the live clock or the scrubber. */
export default function ViewsWorld({ runId, d, defs }: { runId: string; d: BranchDetail; defs: WorldViews }) {
  const { view, screen, setView, setScreen } = useChosen(defs);
  const live = isLive(d.status);
  const screenId = screen?.id ?? null;
  const s = useStops(d, view.id, screenId);
  const sd = useScreen(d.id, view.id, screenId, s.point.at, live, d.events.length);
  const shown = useShown(live, sd, s.play);
  const playing = s.play.playing;
  return (
    <ZoneContext.Provider value={shown.zone}>
      <div className="flex flex-col md:h-[calc(100dvh-48px)] md:min-h-[560px]">
        <header className="flex min-h-11 shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-line px-4 py-1 sm:px-5">
          <h1 className="truncate font-display text-[24px] leading-none" title={defs.world ?? undefined}>{view.label}</h1>
          <StatusChip status={d.status} />
          <ViewPicker views={defs.views} view={view} onView={setView} />
          <ScreenTabs view={view} screen={screen} onScreen={setScreen} />
          <div className="ml-auto flex min-w-0 items-center gap-3">
            {sd.error && <span className="truncate text-xs text-tone-critical" title={sd.error}>✕ {sd.error}</span>}
            <Fallback at={shown.at} />
            <Clock live={live} playing={playing} at={shown.at} fetchedAt={shown.fetchedAt} pending={sd.pending && !playing} />
            <RunMenu runId={runId} d={d} page="world" step={s.point.seq} />
          </div>
        </header>
        {screen
          ? <ViewScreen view={view} screen={screen} data={shown.data} fetchedAt={shown.fetchedAt} />
          : <div className="flex-1 px-5 py-4 text-sm text-muted">This view has no screens.</div>}
        {!live && s.stops.length > 0 && (
          <ViewScrubber stops={s.stops} marks={s.marks} current={s.point.seq} playing={playing} pending={sd.pending}
            rate={s.play.rate} onRate={s.play.setRate}
            onPlay={s.play.toggle} onStep={(seq) => { s.play.leave(); s.setStep(seq); }} onMove={s.move} />
        )}
      </div>
    </ZoneContext.Provider>
  );
}
