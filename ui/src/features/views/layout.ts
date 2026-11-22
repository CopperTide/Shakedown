// A screen's grid: CSS grid-template-areas from the file's `areas`, each name a panel id.
import type { CSSProperties } from 'react';
import type { ScreenDef } from './types';

/** The area rows as CSS wants them: each row quoted, its names split on whitespace. */
export function templateAreas(areas: string[]): string {
  return areas.map((row) => `"${row.trim().split(/\s+/).join(' ')}"`).join(' ');
}

/** The panel ids a screen places, in first-seen order (`.` is an empty cell). */
export function areaNames(screen: Pick<ScreenDef, 'areas'>): string[] {
  const seen: string[] = [];
  for (const row of screen.areas ?? []) {
    for (const name of row.trim().split(/\s+/)) if (name && name !== '.' && !seen.includes(name)) seen.push(name);
  }
  return seen;
}

/** Whether every row names the same number of cells (else CSS drops the whole template). */
export function isRectangular(areas: string[]): boolean {
  const widths = areas.map((r) => r.trim().split(/\s+/).length);
  return widths.every((w) => w === widths[0]);
}

/** The grid's style: the file's areas, columns and rows; a stack of the panels when the areas don't make a grid. */
export function gridStyle(screen: ScreenDef): CSSProperties {
  const areas = screen.areas ?? [];
  if (!areas.length || !isRectangular(areas)) {
    return { gridTemplateAreas: areaNames(screen).map((n) => `"${n}"`).join(' '), gridTemplateColumns: '1fr', gridAutoRows: 'minmax(240px, 1fr)' };
  }
  return {
    gridTemplateAreas: templateAreas(areas),
    gridTemplateColumns: screen.columns ?? `repeat(${areas[0].trim().split(/\s+/).length}, minmax(0, 1fr))`,
    gridTemplateRows: screen.rows ?? `repeat(${areas.length}, minmax(0, 1fr))`,
  };
}

/** On a phone: the panels one under another, in the file's order; figures as tall as they are, the rest a fixed height. */
export function stackedStyle(names: string[], kindOf: (id: string) => string | undefined): CSSProperties {
  return {
    gridTemplateAreas: names.map((n) => `"${n}"`).join(' '),
    gridTemplateColumns: 'minmax(0, 1fr)',
    gridTemplateRows: names.map((n) => (kindOf(n) === 'kpis' ? 'auto' : '420px')).join(' '),
  };
}
