// The map itself (loaded on demand: MapLibre stays out of the main bundle). MapLibre draws the base map;
// a canvas over it draws the view's marks, every frame while the world clock runs.
import maplibregl from 'maplibre-gl';
import 'maplibre-gl/dist/maplibre-gl.css';
import { useEffect, useRef } from 'react';
import { worldNow } from './geo';
import { drawLayers, readPalette } from './mapDraw';
import { type Layer, hasMoving } from './mapLayers';
import { MAP_STYLE, inkMap, readInk } from './mapStyle';
import type { At, MapDef } from './types';

export interface MapViewProps {
  def: MapDef;
  layers: Layer[];
  at: At | null;
  fetchedAt: number;
}

function fitCanvas(c: HTMLCanvasElement): CanvasRenderingContext2D | null {
  const r = c.getBoundingClientRect(), dpr = window.devicePixelRatio || 1;
  const w = Math.round(r.width * dpr), h = Math.round(r.height * dpr);
  if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
  const ctx = c.getContext('2d');
  ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx?.clearRect(0, 0, r.width, r.height);
  return ctx;
}

/** The map, toned to the theme (again when the theme changes). */
function useMap(box: React.RefObject<HTMLDivElement | null>, def: MapDef, onFrame: () => void) {
  const map = useRef<maplibregl.Map | null>(null);
  const frame = useRef(onFrame);
  frame.current = onFrame;
  const center = def.center, zoom = def.zoom;
  useEffect(() => {
    if (!box.current) return;
    const m = new maplibregl.Map({ container: box.current, style: MAP_STYLE, center: center ?? [0, 20], zoom: zoom ?? 2,
      attributionControl: { compact: true }, fadeDuration: 0, dragRotate: false, pitchWithRotate: false });
    m.touchZoomRotate.disableRotation();
    const ink = () => inkMap(m, readInk(document.documentElement));
    m.on('style.load', ink);
    m.on('render', () => frame.current());
    const theme = new MutationObserver(() => { if (m.isStyleLoaded()) ink(); frame.current(); });
    theme.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    map.current = m;
    return () => { theme.disconnect(); m.remove(); map.current = null; };
  }, [box, center, zoom]);
  return map;
}

export default function MapView({ def, layers, at, fetchedAt }: MapViewProps) {
  const box = useRef<HTMLDivElement>(null), canvas = useRef<HTMLCanvasElement>(null);
  const latest = useRef({ layers, at, fetchedAt });
  latest.current = { layers, at, fetchedAt };
  const draw = () => {
    const m = map.current, c = canvas.current;
    if (!m || !c) return;
    const ctx = fitCanvas(c);
    if (!ctx) return;
    const { layers: ls, at: a, fetchedAt: f } = latest.current;
    const now = worldNow(a?.world_ms ?? null, a?.speed ?? 0, f, performance.now()) ?? Date.now();
    drawLayers(ctx, ls, (ll) => { const p = m.project(ll); return [p.x, p.y]; }, readPalette(c), now, m.getZoom());
  };
  const map = useMap(box, def, draw);
  const moving = hasMoving(layers) && (at?.speed ?? 0) > 0;
  const drawRef = useRef(draw);
  drawRef.current = draw;
  useEffect(() => {
    drawRef.current();
    if (!moving) return;
    let raf = 0;
    const tick = () => { drawRef.current(); raf = requestAnimationFrame(tick); };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [map, moving, layers, at, fetchedAt]);
  return (
    <div className="views-map absolute inset-0">
      {/* Inline: maplibre's own (unlayered) CSS makes its container `position: relative`. */}
      <div ref={box} style={{ position: 'absolute', inset: 0 }} />
      <canvas ref={canvas} className="pointer-events-none absolute inset-0 size-full" />
    </div>
  );
}
