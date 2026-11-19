import { describe, expect, it } from 'vitest';
import { diskFill, diskLine } from './status';

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
