// A map panel's layers as marks to draw: points, arcs and moving docs, read by column name.
import { toMs } from './format';
import { type LonLat, placeMoving, route } from './geo';
import { cols } from './rows';
import { type MapData, type MapDef, type MapLayerDef, type Rows, type Tone, asTone } from './types';

export interface PointMark { key: string; label: string; at: LonLat; tone: Tone; chip: string }
export interface PathMark { key: string; label: string; o: LonLat; d: LonLat; depart: number | null; arrive: number | null; tone: Tone; chip: string }

export type Layer =
  | { id: string; kind: 'points'; marks: PointMark[] }
  | { id: string; kind: 'arcs' | 'moving'; marks: PathMark[] };

const finite = (...xs: (number | null)[]) => xs.every((x) => x != null && Number.isFinite(x));

function points(r: Rows): PointMark[] {
  const c = cols(r.columns);
  const out: PointMark[] = [];
  for (const row of r.rows) {
    const lat = c.num(row, 'lat'), lon = c.num(row, 'lon');
    if (!finite(lat, lon)) continue;
    out.push({ key: c.str(row, 'key'), label: c.str(row, 'label') || c.str(row, 'key'), at: [lon!, lat!], tone: asTone(c.get(row, 'tone')), chip: c.str(row, 'chip') });
  }
  return out;
}

function paths(r: Rows): PathMark[] {
  const c = cols(r.columns);
  const out: PathMark[] = [];
  for (const row of r.rows) {
    const olat = c.num(row, 'olat'), olon = c.num(row, 'olon'), dlat = c.num(row, 'dlat'), dlon = c.num(row, 'dlon');
    if (!finite(olat, olon, dlat, dlon)) continue;
    out.push({
      key: c.str(row, 'key'), label: c.str(row, 'label'), o: [olon!, olat!], d: [dlon!, dlat!],
      depart: toMs(c.get(row, 'depart')), arrive: toMs(c.get(row, 'arrive')), tone: asTone(c.get(row, 'tone')), chip: c.str(row, 'chip'),
    });
  }
  return out;
}

/** The kind of a layer core sent: the def's, else what its columns look like. */
function kindOf(def: MapLayerDef | undefined, r: Rows): Layer['kind'] {
  if (def) return def.kind;
  if (r.columns.includes('lat')) return 'points';
  return r.columns.includes('depart') ? 'moving' : 'arcs';
}

/** The layers in the def's order (then any the def doesn't name), drawn bottom to top: arcs under points under moving docs. */
export function mapLayers(def: MapDef, data: MapData): Layer[] {
  const got = data.layers ?? {};
  const ids = [...(def.layers ?? []).map((l) => l.id), ...Object.keys(got).filter((k) => !(def.layers ?? []).some((l) => l.id === k))];
  const out: Layer[] = [];
  for (const id of ids) {
    const r = got[id];
    if (!r) continue;
    const kind = kindOf(def.layers?.find((l) => l.id === id), r);
    out.push(kind === 'points' ? { id, kind, marks: points(r) } : { id, kind, marks: paths(r) });
  }
  const rank = { arcs: 0, points: 1, moving: 2 } as const;
  return out.sort((a, b) => rank[a.kind] - rank[b.kind]);
}

/** Whether any layer has docs that move with the clock. */
export const hasMoving = (layers: Layer[]) => layers.some((l) => l.kind === 'moving' && l.marks.length > 0);

const routes = new Map<string, LonLat[]>();

/** An arc's great circle (cached by its ends). */
export function arcRoute(m: PathMark): LonLat[] {
  const k = `${m.o[0]},${m.o[1]},${m.d[0]},${m.d[1]}`;
  let r = routes.get(k);
  if (!r) {
    if (routes.size > 5000) routes.clear();
    r = route(m.o, m.d, 24);
    routes.set(k, r);
  }
  return r;
}

/** A moving doc's place and heading at world time `now`. */
export const movingAt = (m: PathMark, now: number) => placeMoving(m.o, m.d, m.depart, m.arrive, now);

/** "14 p1 · 20 m1": how many marks each layer has, by layer id. */
export const layerSummary = (layers: Layer[]) => layers.map((l) => `${l.marks.length.toLocaleString('en-US')} ${l.id}`).join(' · ');
