// A series' colour follows the entity, never its rank: each name takes the slot of its place in
// the list of every name in the range (the summary's options), so picking a filter never repaints
// the others. Five slots in the tokens' order; a name past them is drawn in the "other" grey.

export const SLOTS = 5;

/** The CSS colour of `name` among `all`: `var(--cat-n)`, or the "other" grey. */
export function colorOf(name: string, all: string[]): string {
  const i = all.indexOf(name);
  return i >= 0 && i < SLOTS ? `var(--cat-${i + 1})` : 'var(--cat-other)';
}

/** A `var(--x)` colour as the theme resolves it now (a canvas needs the value). */
export function resolved(color: string, el: Element = document.documentElement): string {
  const m = /^var\((--[\w-]+)\)$/.exec(color);
  return m ? getComputedStyle(el).getPropertyValue(m[1]).trim() || '#8c8479' : color;
}
