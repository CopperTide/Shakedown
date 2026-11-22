// The base map in the page's warm grays: every OpenFreeMap positron layer repainted from the theme's
// map tokens, so the marks on top carry the only colour.
export const MAP_STYLE = 'https://tiles.openfreemap.org/styles/positron';

export interface MapInk {
  land: string;
  water: string;
  road: string;
  boundary: string;
  label: string;
}

export function readInk(el: Element): MapInk {
  const css = getComputedStyle(el);
  const v = (n: string, f: string) => css.getPropertyValue(n).trim() || f;
  return { land: v('--map-land', '#eeeae3'), water: v('--map-water', '#d9d4cb'), road: v('--map-road', '#f8f6f2'), boundary: v('--map-boundary', '#b9b0a3'), label: v('--map-label', '#857d72') };
}

type Paint = [string, unknown][];

/** The paint for one style layer, by its type and id; null leaves it as it is. */
export function inkFor(type: string, id: string, ink: MapInk): Paint | null {
  const water = /water|ocean|river|lake/.test(id);
  if (type === 'background') return [['background-color', ink.land]];
  if (type === 'fill') return water ? [['fill-color', ink.water], ['fill-outline-color', ink.water]] : [['fill-color', ink.land], ['fill-outline-color', ink.land]];
  if (type === 'line') {
    if (water) return [['line-color', ink.water]];
    if (/boundary|admin/.test(id)) return [['line-color', ink.boundary]];
    return [['line-color', ink.road]];
  }
  if (type === 'symbol') return [['text-color', ink.label], ['text-halo-color', ink.land], ['text-opacity', 0.8], ['icon-opacity', 0.5]];
  return null;
}

interface StyleLike {
  getStyle: () => { layers?: { id: string; type: string }[] } | undefined;
  setPaintProperty: (id: string, name: string, value: unknown) => void;
}

/** Repaints the loaded style's layers in the theme's map ink. */
export function inkMap(map: StyleLike, ink: MapInk) {
  for (const l of map.getStyle()?.layers ?? []) {
    for (const [name, value] of inkFor(l.type, l.id, ink) ?? []) {
      try { map.setPaintProperty(l.id, name, value); } catch { /* a property this layer doesn't take */ }
    }
  }
}
