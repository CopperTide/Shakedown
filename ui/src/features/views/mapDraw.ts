// Drawing a map panel's marks on a canvas over the map, given how to project a point to the screen.
import type { LonLat } from './geo';
import { type Layer, type PathMark, type PointMark, arcRoute, movingAt } from './mapLayers';
import { toneVar } from './tones';
import { TONES, type Tone } from './types';

export type Project = (ll: LonLat) => [number, number];

export interface Palette {
  tone: Record<Tone, string>;
  fg: string;
  muted: string;
  halo: string;
}

/** The palette from the page's CSS variables (so the theme applies). */
export function readPalette(el: Element): Palette {
  const css = getComputedStyle(el);
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  const tone = Object.fromEntries(TONES.map((t) => [t, v(toneVar(t), '#8c8479')])) as Record<Tone, string>;
  return { tone, fg: v('--fg', '#1b1815'), muted: v('--muted', '#514a42'), halo: v('--map-land', '#eeeae3') };
}

const MONO = "'IBM Plex Mono', ui-monospace, monospace";

function label(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, color: string, halo: string, weight = 500) {
  ctx.font = `${weight} 10.5px ${MONO}`;
  ctx.lineWidth = 3;
  ctx.strokeStyle = halo;
  ctx.lineJoin = 'round';
  ctx.strokeText(text, x, y);
  ctx.fillStyle = color;
  ctx.fillText(text, x, y);
}

function drawArcs(ctx: CanvasRenderingContext2D, marks: PathMark[], p: Project, pal: Palette) {
  ctx.lineWidth = 1;
  for (const m of marks) {
    const pts = arcRoute(m).map(p);
    ctx.beginPath();
    pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
    ctx.globalAlpha = m.tone === 'neutral' ? 0.28 : 0.75;
    ctx.strokeStyle = pal.tone[m.tone];
    ctx.lineWidth = m.tone === 'neutral' ? 0.8 : 1.4;
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
}

function drawPoints(ctx: CanvasRenderingContext2D, marks: PointMark[], p: Project, pal: Palette, zoom: number) {
  // Neutral marks first, so a toned one is never hidden under them.
  const sorted = [...marks].sort((a, b) => Number(a.tone !== 'neutral') - Number(b.tone !== 'neutral'));
  for (const m of sorted) {
    const [x, y] = p(m.at);
    const toned = m.tone !== 'neutral';
    ctx.beginPath();
    ctx.arc(x, y, toned ? 3.2 : 2.2, 0, Math.PI * 2);
    ctx.fillStyle = toned ? pal.tone[m.tone] : pal.muted;
    ctx.fill();
    if (toned) {
      ctx.beginPath();
      ctx.arc(x, y, 7, 0, Math.PI * 2);
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = pal.tone[m.tone];
      ctx.stroke();
      label(ctx, m.chip ? `${m.label} ${m.chip}` : m.label, x + 10, y + 3.5, pal.tone[m.tone], pal.halo, 600);
    } else if (zoom >= 4.5) {
      label(ctx, m.label, x + 5, y + 3.5, pal.muted, pal.halo);
    }
  }
}

/** A chevron pointing along `heading` (degrees from north). */
function chevron(ctx: CanvasRenderingContext2D, x: number, y: number, heading: number, size: number, color: string, halo: string) {
  ctx.save();
  ctx.translate(x, y);
  ctx.rotate((heading * Math.PI) / 180);
  ctx.beginPath();
  ctx.moveTo(0, -size);
  ctx.lineTo(size * 0.72, size * 0.8);
  ctx.lineTo(0, size * 0.36);
  ctx.lineTo(-size * 0.72, size * 0.8);
  ctx.closePath();
  ctx.lineWidth = 1.5;
  ctx.strokeStyle = halo;
  ctx.stroke();
  ctx.fillStyle = color;
  ctx.fill();
  ctx.restore();
}

/** Moving docs named on the map: only the ones that need attention (an active one is just its glyph). */
const LABELLED = new Set<Tone>(['warn', 'critical', 'accent']);

function drawMoving(ctx: CanvasRenderingContext2D, marks: PathMark[], p: Project, pal: Palette, now: number, zoom: number) {
  const sorted = [...marks].sort((a, b) => Number(a.tone !== 'neutral') - Number(b.tone !== 'neutral'));
  for (const m of sorted) {
    const placed = movingAt(m, now);
    const [x, y] = p(placed.at);
    // The screen heading: the projected direction a little way ahead (Mercator bends bearings).
    const h = (placed.heading * Math.PI) / 180, k = Math.max(0.05, Math.cos((placed.at[1] * Math.PI) / 180));
    const [ax, ay] = p([placed.at[0] + (Math.sin(h) * 0.2) / k, placed.at[1] + Math.cos(h) * 0.2]);
    const heading = (Math.atan2(ax - x, -(ay - y)) * 180) / Math.PI;
    const toned = m.tone !== 'neutral';
    chevron(ctx, x, y, heading, toned ? 6 : 4.6, toned ? pal.tone[m.tone] : pal.muted, pal.halo);
    if (LABELLED.has(m.tone) && zoom >= 3.5 && m.label) label(ctx, m.chip ? `${m.label} ${m.chip}` : m.label, x + 8, y - 6, pal.tone[m.tone], pal.halo, 600);
  }
}

/** One frame of the overlay at world time `now`. */
export function drawLayers(ctx: CanvasRenderingContext2D, layers: Layer[], p: Project, pal: Palette, now: number, zoom: number) {
  for (const l of layers) {
    if (l.kind === 'arcs') drawArcs(ctx, l.marks, p, pal);
    else if (l.kind === 'points') drawPoints(ctx, l.marks, p, pal, zoom);
    else drawMoving(ctx, l.marks, p, pal, now, zoom);
  }
}
