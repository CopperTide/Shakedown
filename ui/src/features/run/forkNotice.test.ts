import { describe, expect, it } from 'vitest';
import { forkNotice } from './forkNotice';

const r = { branch_id: 'b2', start_seq: 7, moved_from: null, reason: null };

describe('fork notice', () => {
  it('says nothing for a plain fork', () => {
    expect(forkNotice(r)).toBeNull();
  });

  it('says where the rerun started when it moved', () => {
    expect(forkNotice({ ...r, moved_from: 5, reason: 'the agent was mid-turn' })).toBe('Restarted from step 7 instead of 5: the agent was mid-turn');
  });

  it("says the team's driver was started again on the restored world", () => {
    expect(forkNotice({ ...r, driver: 'relaunched' })).toBe('Driver restarted at step 7.');
    expect(forkNotice({ ...r, moved_from: 5, reason: 'x', driver: 'failed' })).toBe("Restarted from step 7 instead of 5: x Driver didn't restart: the rerun failed.");
  });
});
