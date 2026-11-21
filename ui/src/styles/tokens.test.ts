// The design tokens follow the v7 identity rules in both themes (spec §4).
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const css = readFileSync(new URL('./index.css', import.meta.url), 'utf8');

/** The declarations inside the first block that starts with `selector {`. */
function block(selector: string): string {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) return '';
  return css.slice(start, css.indexOf('\n}', start));
}

const NAMES = ['--bg', '--surface', '--raised', '--sunken', '--hover', '--line', '--line-strong', '--fg', '--muted',
  '--faint', '--accent', '--primary', '--primary-hover', '--primary-fg', '--alarm', '--warn', '--ok', '--copper'];

describe('design tokens', () => {
  it('tokens_defined_for_both_themes', () => {
    const light = block(':root');
    const dark = block(":root[data-theme='dark']");
    for (const name of NAMES) {
      expect(light, `${name} in light`).toContain(`${name}:`);
      expect(dark, `${name} in dark`).toContain(`${name}:`);
    }
  });

  it('no_inter', () => {
    expect(css).not.toMatch(/Inter/);
  });

  it('light_palette', () => {
    const light = block(':root');
    expect(light).toContain('--surface: #fdfcfa');
    expect(light).toContain('--bg: #f6f4f0');
    expect(light).toContain('--fg: #1b1815');
    expect(light).toContain('--copper: #ee7a3a');
  });

  it('fonts_follow_the_identity', () => {
    expect(css).toContain("--font-sans: 'Instrument Sans Variable'");
    expect(css).toContain("--font-display: 'Instrument Serif'");
    expect(css).toContain("--font-mono: 'IBM Plex Mono'");
  });
});
