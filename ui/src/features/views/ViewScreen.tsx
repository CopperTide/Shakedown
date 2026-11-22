import { useSyncExternalStore } from 'react';
import BarsPanel from './BarsPanel';
import BoardPanel from './BoardPanel';
import FeedPanel from './FeedPanel';
import KpisPanel from './KpisPanel';
import { areaNames, gridStyle, stackedStyle } from './layout';
import MapPanel from './MapPanel';
import { PanelErrorBody, PanelFrame } from './PanelFrame';
import TimelinePanel from './TimelinePanel';
import {
  type At, type BarsData, type BoardData, type FeedData, type KpisData, type MapData, type PanelData, type PanelDef, type PanelError,
  type ScreenData, type ScreenDef, type TimelineData, type ViewDef, isPanelError,
} from './types';

export interface PanelProps {
  id: string;
  def: PanelDef;
  data: PanelData;
  at: At | null;
  fetchedAt: number;
}

/** One panel, by its kind. */
export function PanelBody({ id, def, data, at, fetchedAt }: PanelProps) {
  switch (def.kind) {
    case 'board': return <BoardPanel id={id} def={def} data={data as BoardData} />;
    case 'timeline': return <TimelinePanel id={id} def={def} data={data as TimelineData} now={at?.world_ms ?? null} />;
    case 'map': return <MapPanel id={id} def={def} data={data as MapData} at={at} fetchedAt={fetchedAt} />;
    case 'kpis': return <PanelFrame id={id} title={def.title}><KpisPanel def={def} data={data as KpisData} /></PanelFrame>;
    case 'bars': return <PanelFrame id={id} title={def.title} meta={(data as BarsData).rows.length}><BarsPanel def={def} data={data as BarsData} /></PanelFrame>;
    case 'feed': return <PanelFrame id={id} title={def.title} meta={(data as FeedData).rows.length}><FeedPanel data={data as FeedData} at={at} /></PanelFrame>;
    default: return <PanelFrame id={id} title={(def as PanelDef).title}><PanelErrorBody error={`This renderer draws no ${(def as { kind: string }).kind} panels.`} /></PanelFrame>;
  }
}

/** What a panel's slot holds: its data, an error for it alone, or nothing yet. */
export function slotOf(def: PanelDef | undefined, got: PanelData | PanelError | undefined, loaded: boolean): { error: string } | { data: PanelData } | null {
  if (!def) return { error: 'The view has no panel by this name.' };
  if (!got) return loaded ? { error: 'Core sent nothing for this panel.' } : null;
  if (isPanelError(got)) return { error: got.error };
  if (got.kind && got.kind !== def.kind) return { error: `Core sent a ${got.kind} for a ${def.kind} panel.` };
  return { data: got };
}

const PHONE = '(max-width: 767px)';
const subscribeNarrow = (on: () => void) => {
  const mq = window.matchMedia(PHONE);
  mq.addEventListener('change', on);
  return () => mq.removeEventListener('change', on);
};

/** Whether the window is phone-wide (the panels stack). */
function useNarrow(): boolean {
  return useSyncExternalStore(subscribeNarrow, () => window.matchMedia(PHONE).matches, () => false);
}

/** A screen of a view: its panels on the file's grid, ruled by hairlines. */
export default function ViewScreen({ view, screen, data, fetchedAt }: { view: ViewDef; screen: ScreenDef; data: ScreenData | null; fetchedAt: number }) {
  const at = data?.at ?? null;
  const narrow = useNarrow();
  const names = areaNames(screen);
  const style = narrow ? stackedStyle(names, (id) => view.panels?.[id]?.kind) : gridStyle(screen);
  return (
    <div className="grid min-h-0 flex-1 gap-px border-y border-line bg-line" style={style}>
      {names.map((id) => {
        const def = view.panels?.[id];
        const slot = slotOf(def, data?.panels?.[id], data != null);
        if (slot && 'data' in slot && def) return <PanelBody key={id} id={id} def={def} data={slot.data} at={at} fetchedAt={fetchedAt} />;
        return (
          <PanelFrame key={id} id={id} title={def?.title ?? id}>
            {slot && 'error' in slot ? <PanelErrorBody error={slot.error} /> : <div className="px-3 py-2.5 text-xs text-faint">Loading…</div>}
          </PanelFrame>
        );
      })}
    </div>
  );
}
