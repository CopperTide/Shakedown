import { describe, expect, it } from 'vitest';
import { exportedLine } from './RunMenu';

describe('exportedLine', () => {
  it('says where to add a target when none is set up', () => {
    expect(exportedLine({ branch_id: 'b', exports: [] })).toMatch(/Settings → Observability/);
  });
  it('lists each target with what happened', () => {
    const line = exportedLine({ branch_id: 'b', exports: [
      { target_id: '1', target: 'Langfuse', state: 'done', spans: 12 },
      { target_id: '2', target: 'Phoenix', state: 'failed', error: '401' },
      { target_id: '3', target: 'Tempo', state: 'queued' },
    ] });
    expect(line).toBe('Langfuse: sent 12 spans · Phoenix: failed (401) · Tempo: queued until the run ends');
  });
});
