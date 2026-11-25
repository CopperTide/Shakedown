// A world's declarative views (`shakedown-view/1`) as core sends them: the parsed files, and one screen
// evaluated at a point of a branch. The renderer knows only these shapes, never what a world is about.

export type Tone = 'neutral' | 'active' | 'done' | 'warn' | 'critical' | 'accent';
export const TONES: readonly Tone[] = ['neutral', 'active', 'done', 'warn', 'critical', 'accent'];

export type Format = 'text' | 'int' | 'number' | 'percent' | 'ratio_pct' | 'usd' | 'minutes' | 'time' | 'chip';

/** A dotted path into a doc, or the first of several present. */
export type Field = string | string[];

export interface StyleRule {
  where?: Record<string, unknown>;
  tone: Tone;
  label?: string;
}

export interface KpiItemDef {
  label: string;
  format?: Format;
  decimals?: number;
  [k: string]: unknown;
}

export interface BoardColumnDef {
  field: Field;
  label: string;
  format?: Format;
  decimals?: number;
  style?: string | StyleRule[];
}

export interface BoardFilterDef {
  id: string;
  label: string;
  where?: Record<string, unknown>;
}

export interface MapLayerDef {
  id: string;
  kind: 'points' | 'arcs' | 'moving';
  label?: Field;
  [k: string]: unknown;
}

interface PanelBase {
  title?: string;
  [k: string]: unknown;
}

export interface KpisDef extends PanelBase { kind: 'kpis'; items: KpiItemDef[] }
export interface MapDef extends PanelBase { kind: 'map'; center?: [number, number]; zoom?: number; layers: MapLayerDef[] }
export interface BoardDef extends PanelBase {
  kind: 'board';
  key?: Field;
  columns: BoardColumnDef[];
  sort?: { field: Field; desc?: boolean };
  filters?: BoardFilterDef[];
  top?: number;
}
export interface TimelineDef extends PanelBase { kind: 'timeline' }
export interface BarsDef extends PanelBase { kind: 'bars'; unit?: string; format?: Format; decimals?: number }
export interface FeedDef extends PanelBase { kind: 'feed' }
/** A conversation; `right` are the senders drawn on the right. */
export interface ChatDef extends PanelBase { kind: 'chat'; right?: string[] }

export type PanelDef = KpisDef | MapDef | BoardDef | TimelineDef | BarsDef | FeedDef | ChatDef;
export type PanelKind = PanelDef['kind'];

export interface ScreenDef {
  id: string;
  label: string;
  areas: string[];
  columns?: string;
  rows?: string;
}

export interface ViewDef {
  id: string;
  label: string;
  part?: string;
  styles?: Record<string, StyleRule[]>;
  panels: Record<string, PanelDef>;
  screens: ScreenDef[];
}

export interface WorldViews {
  world: string | null;
  views: ViewDef[];
}

/** Rows of arrays under named columns: read by name, so core can add columns. */
export interface Rows {
  columns: string[];
  rows: unknown[][];
}

export type AtPoint = 'live' | 'final' | 'step';

export interface At {
  seq: number | null;
  point: AtPoint;
  clock_ms: number | null;
  world_ms: number | null;
  speed: number;
  paused: boolean;
  /** The world's UTC offset in minutes: its times are shown local with it (absent or null: UTC). */
  utc_offset_min?: number | null;
  /** Set when the step asked for couldn't be restored and an earlier one is shown. */
  fallback?: { asked: number; why: string };
}

export interface KpisData { kind: 'kpis'; items: { value: unknown; tone?: Tone; chip?: string | null }[] }
export interface MapData { kind: 'map'; layers: Record<string, Rows> }
export interface BoardData extends Rows { kind: 'board'; total?: number }
export interface TimelineData extends Rows { kind: 'timeline'; lanes: string[]; range: [number, number] }
export interface BarsData extends Rows { kind: 'bars' }
export interface FeedData extends Rows { kind: 'feed' }
/** Messages oldest first under `key from text tone chip`; `total` before `top` cut it. */
export interface ChatData extends Rows { kind: 'chat'; right?: string[]; total?: number }
export interface PanelError { kind: string; error: string }

export type PanelData = KpisData | MapData | BoardData | TimelineData | BarsData | FeedData | ChatData;

export interface ScreenData {
  view: string;
  screen: string;
  at: At;
  panels: Record<string, PanelData | PanelError>;
}

export const isPanelError = (p: unknown): p is PanelError =>
  typeof p === 'object' && p != null && typeof (p as PanelError).error === 'string';

export const asTone = (v: unknown): Tone => (TONES.includes(v as Tone) ? (v as Tone) : 'neutral');
