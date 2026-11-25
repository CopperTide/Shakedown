// The status bar's lines: disk use, and the warning when nothing can run an agent.
import type { Health, ProviderStatus } from '@/model/types';

const gb = (n: number) => {
  const v = n / 2 ** 30;
  return v >= 10 ? Math.round(v).toString() : (Math.round(v * 10) / 10).toString();
};

/** "Saves 0.4 / 5 GB" (every save the shakedown keeps; GB = 2^30 bytes); empty when the server doesn't report disk use. */
export function diskLine(h: Health | null): string {
  if (!h?.disk) return '';
  return `Saves ${gb(h.disk.bytes)} / ${gb(h.disk.limit_bytes)} GB`;
}

/** How full the saves are, from 0 to 1, and whether new runs are refused; null without disk info. */
export function diskFill(h: Health | null): { fraction: number; full: boolean } | null {
  if (!h?.disk || h.disk.limit_bytes <= 0) return null;
  return { fraction: Math.min(1, h.disk.bytes / h.disk.limit_bytes), full: h.disk.bytes >= h.disk.limit_bytes };
}

/**
 * The status bar's warning when nothing can run an agent, else null. `providers` are the caller's
 * (`/api/providers`, their own keys applied); null while unknown, which warns of nothing. A CLI on
 * this machine's PATH runs agents locally; on a shaked whose runs go to sandboxes the CLI agents run
 * there, so only a model key counts.
 */
export function noModelLine(h: Health, providers: ProviderStatus[] | null): string | null {
  if (!providers || providers.some((s) => s.available)) return null;
  if (h.sandboxes) return 'No model to run agents with: no API key is set.';
  if (h.claude || h.codex) return null;
  return 'No model to run agents with: No agent CLI on PATH, and no API key is set.';
}
