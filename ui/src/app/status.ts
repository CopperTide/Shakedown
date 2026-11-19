// The status bar's lines: disk use and the agent CLIs found.
import type { Health } from '@/model/types';

const gb = (n: number) => {
  const v = n / 2 ** 30;
  return v >= 10 ? Math.round(v).toString() : (Math.round(v * 10) / 10).toString();
};

/** "Saves 0.4 / 5 GB" (every save the arena keeps; GB = 2^30 bytes); empty when the server doesn't report disk use. */
export function diskLine(h: Health | null): string {
  if (!h?.disk) return '';
  return `Saves ${gb(h.disk.bytes)} / ${gb(h.disk.limit_bytes)} GB`;
}

/** How full the saves are, from 0 to 1, and whether new runs are refused; null without disk info. */
export function diskFill(h: Health | null): { fraction: number; full: boolean } | null {
  if (!h?.disk || h.disk.limit_bytes <= 0) return null;
  return { fraction: Math.min(1, h.disk.bytes / h.disk.limit_bytes), full: h.disk.bytes >= h.disk.limit_bytes };
}

/** The CLI modes found on this machine; API mode's providers have their own line. */
export function modesLine(h: Health | null): string {
  if (!h) return '';
  const found = [h.claude && 'Claude Code', h.codex && 'Codex'].filter(Boolean).join(' · ');
  return found || 'No agent CLI on PATH';
}
