import { describe, expect, it } from 'vitest';
import type { ProviderStatus } from '@/model/types';
import { diskFill, diskLine, noModelLine } from './status';

const GB = 2 ** 30;
const health = (bytes: number, limit = 5 * GB) => ({
  open_available: true, claude: false, codex: false, disk: { bytes, limit_bytes: limit },
});

describe('diskLine', () => {
  it('shows the saves used and the limit in GB', () => {
    expect(diskLine(health(0.4 * GB))).toBe('Saves 0.4 / 5 GB');
    expect(diskLine(health(12 * GB, 20 * GB))).toBe('Saves 12 / 20 GB');
  });
  it('is empty without disk info', () => {
    expect(diskLine(null)).toBe('');
    expect(diskLine({ open_available: true, claude: false, codex: false })).toBe('');
  });
});

describe('disk fill', () => {
  it('says how full the saves are and when new runs are refused', () => {
    expect(diskFill(health(GB, 4 * GB))).toEqual({ fraction: 0.25, full: false });
    expect(diskFill(health(5 * GB, 4 * GB))).toEqual({ fraction: 1, full: true });
    expect(diskFill(null)).toBeNull();
  });
});

describe('noModelLine', () => {
  const p = (available: boolean): ProviderStatus => ({ id: 'openrouter', kind: 'openrouter', label: 'OpenRouter', available, checked: false });
  const h = (more: object = {}) => ({ open_available: false, claude: false, codex: false, ...more });

  it('says nothing when the caller has a ready provider (their own key), or before their list is known', () => {
    expect(noModelLine(h(), [p(true)])).toBeNull();
    expect(noModelLine(h({ sandboxes: true }), [p(true)])).toBeNull();
    expect(noModelLine(h(), null)).toBeNull();
  });
  it('locally, a CLI on PATH is enough; without one or a key it warns', () => {
    expect(noModelLine(h({ claude: true }), [p(false)])).toBeNull();
    expect(noModelLine(h(), [p(false)])).toBe('No model to run agents with: No agent CLI on PATH, and no API key is set.');
  });
  it('on a sandboxed shaked never blames the PATH: only a key counts', () => {
    expect(noModelLine(h({ sandboxes: true }), [p(false)])).toBe('No model to run agents with: no API key is set.');
    expect(noModelLine(h({ sandboxes: true, claude: true }), [])).toBe('No model to run agents with: no API key is set.');
  });
});
