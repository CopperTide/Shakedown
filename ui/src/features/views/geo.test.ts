import { describe, expect, it } from 'vitest';
import { along, bearing, fraction, placeMoving, route, worldNow } from './geo';
import { arcRoute, hasMoving, layerSummary, mapLayers, movingAt } from './mapLayers';
import { inkFor } from './mapStyle';
import type { MapData, MapDef } from './types';

const close = (a: number, b: number, eps = 1e-6) => Math.abs(a - b) < eps;

describe('great circles', () => {
  it('along_starts_and_ends_at_the_ends', () => {
    const a: [number, number] = [0, 0], b: [number, number] = [90, 0];
    expect(along(a, b, 0).every((v, i) => close(v, a[i]))).toBe(true);
    expect(along(a, b, 1).every((v, i) => close(v, b[i]))).toBe(true);
    expect(along(a, b, 0.5)[0]).toBeCloseTo(45);
  });

  it('a_great_circle_bulges_toward_the_pole', () => {
    const mid = along([-60, 40], [60, 40], 0.5);
    expect(mid[0]).toBeCloseTo(0);
    expect(mid[1]).toBeGreaterThan(40);
  });

  it('route_unwraps_the_antimeridian', () => {
    const pts = route([170, 10], [-170, 10], 8);
    expect(pts).toHaveLength(9);
    for (let i = 1; i < pts.length; i++) expect(Math.abs(pts[i][0] - pts[i - 1][0])).toBeLessThan(180);
    expect(pts[8][0]).toBeCloseTo(190);
  });

  it('bearing_in_degrees_from_north', () => {
    expect(bearing([0, 0], [0, 10])).toBeCloseTo(0);
    expect(bearing([0, 0], [10, 0])).toBeCloseTo(90);
    expect(bearing([0, 10], [0, 0])).toBeCloseTo(180);
    expect(bearing([10, 0], [0, 0])).toBeCloseTo(270);
  });

  it('fraction_clamps_and_needs_an_interval', () => {
    expect(fraction(15, 10, 20)).toBe(0.5);
    expect(fraction(5, 10, 20)).toBe(0);
    expect(fraction(25, 10, 20)).toBe(1);
    expect(fraction(15, null, 20)).toBe(0);
    expect(fraction(15, 20, 20)).toBe(0);
  });

  it('a_moving_doc_heads_along_its_path', () => {
    const p = placeMoving([0, 0], [10, 0], 0, 100, 50);
    expect(p.t).toBe(0.5);
    expect(p.at[0]).toBeCloseTo(5);
    expect(p.heading).toBeCloseTo(90, 0);
    expect(placeMoving([0, 0], [10, 0], 0, 100, 500).heading).toBeCloseTo(90, 0);
  });

  it('world_now_runs_on_at_speed', () => {
    expect(worldNow(1000, 60, 100, 200)).toBe(1000 + 60 * 100);
    expect(worldNow(1000, 0, 100, 200)).toBe(1000);
    expect(worldNow(1000, 60, 300, 200)).toBe(1000);
    expect(worldNow(null, 60, 0, 1)).toBeNull();
  });
});

const def: MapDef = { kind: 'map', center: [0, 0], zoom: 3, layers: [{ id: 'm1', kind: 'moving' }, { id: 'p1', kind: 'points' }, { id: 'a1', kind: 'arcs' }] };
const PATH = ['key', 'label', 'olat', 'olon', 'dlat', 'dlon', 'depart', 'arrive', 'tone', 'chip'];
const data: MapData = {
  kind: 'map',
  layers: {
    p1: { columns: ['lon', 'key', 'lat', 'tone', 'label', 'chip'], rows: [[1, 's1', 2, 'warn', 'S1', 'Slow'], [null, 's2', 3, 'neutral', '', null]] },
    a1: { columns: PATH, rows: [['x1', 'X1', 0, 0, 0, 10, null, null, 'neutral', null]] },
    m1: { columns: PATH, rows: [['v1', 'V1', 0, 0, 0, 10, 0, 100, 'active', null]] },
    extra: { columns: ['key', 'lat', 'lon'], rows: [['e1', 1, 1]] },
  },
};

describe('map layers', () => {
  it('reads_layers_by_name_and_orders_them_for_drawing', () => {
    const ls = mapLayers(def, data);
    expect(ls.map((l) => `${l.kind}:${l.id}`)).toEqual(['arcs:a1', 'points:p1', 'points:extra', 'moving:m1']);
    const p = ls[1];
    expect(p.kind === 'points' && p.marks).toEqual([{ key: 's1', label: 'S1', at: [1, 2], tone: 'warn', chip: 'Slow' }]);
    expect(hasMoving(ls)).toBe(true);
    expect(layerSummary(ls)).toBe('1 a1 · 1 p1 · 1 extra · 1 m1');
  });

  it('places_a_moving_mark_by_the_clock', () => {
    const m = mapLayers(def, data).find((l) => l.id === 'm1');
    if (m?.kind !== 'moving') throw new Error('no moving layer');
    expect(movingAt(m.marks[0], 25).at[0]).toBeCloseTo(2.5);
    expect(arcRoute(m.marks[0])).toHaveLength(25);
  });

  it('a_missing_layer_is_skipped', () => {
    expect(mapLayers(def, { kind: 'map', layers: {} })).toEqual([]);
  });
});

describe('base map ink', () => {
  const ink = { land: 'L', water: 'W', road: 'R', boundary: 'B', label: 'T' };
  it('repaints_by_layer_type_and_id', () => {
    expect(inkFor('background', 'background', ink)).toEqual([['background-color', 'L']]);
    expect(inkFor('fill', 'water', ink)?.[0]).toEqual(['fill-color', 'W']);
    expect(inkFor('fill', 'park', ink)?.[0]).toEqual(['fill-color', 'L']);
    expect(inkFor('line', 'boundary_country', ink)).toEqual([['line-color', 'B']]);
    expect(inkFor('line', 'highway_major', ink)).toEqual([['line-color', 'R']]);
    expect(inkFor('symbol', 'place_city', ink)?.[0]).toEqual(['text-color', 'T']);
    expect(inkFor('raster', 'x', ink)).toBeNull();
  });
});
