import { describe, expect, it } from 'vitest';
import { teamForRun } from '@/model/team';
import { checkBody, emptyForm, formProblem } from './connect';
import { type Connection, codeBody, codeTeam, parseEnv, splitCommand, testLine } from './connections';

const conn: Connection = {
  id: 'c1', name: 'LangGraph team', kind: 'code', command: ['uv', 'run', 'python', 'driver.py'], workdir: '/work', env: { MODE: 'fast' }, created_at: '2026-11-17T12:00:00Z',
};

describe('Code connections', () => {
  it('parses env lines, skips comments, and names a bad line', () => {
    expect(parseEnv('# the team\nOPENAI_MODEL=gpt-4.1-mini\n\nMODE = fast')).toEqual({ env: { OPENAI_MODEL: 'gpt-4.1-mini', MODE: 'fast' } });
    expect(parseEnv('A=1\nNOVALUE')).toEqual({ error: 'Environment line 2 isn’t “NAME=value”.' });
    expect(parseEnv('A=1\nA=2')).toEqual({ error: 'The variable A is there twice.' });
  });

  it('splits a command line like a shell would, quotes kept together', () => {
    expect(splitCommand('uv run python "my driver.py" --task \'27 b\'')).toEqual(['uv', 'run', 'python', 'my driver.py', '--task', '27 b']);
    expect(splitCommand('  ')).toEqual([]);
  });

  it('needs a name and a command before it can be tested', () => {
    const f = { ...emptyForm('code'), name: 'Team' };
    expect(formProblem(f)).toBe('Give the command that starts your driver.');
    expect(formProblem({ ...f, command: 'python driver.py', name: ' ' })).toBe('Name the connection.');
    expect(formProblem({ ...f, command: 'python driver.py', env: 'BAD' })).toBe('Environment line 1 isn’t “NAME=value”.');
    expect(formProblem({ ...f, command: 'python driver.py' })).toBeNull();
    expect(checkBody({ ...f, command: 'python driver.py' })).toBeNull();
    expect(codeBody({ ...f, command: 'python driver.py', workdir: ' /work ', env: 'MODE=fast' })).toEqual({ name: 'Team', command: ['python', 'driver.py'], workdir: '/work', env: { MODE: 'fast' } });
  });

  it('makes a team that posts the connection', () => {
    const t = codeTeam(conn);
    expect(t).toMatchObject({ connection: 'c1', runner: 'external', agents: [] });
    expect(teamForRun(t, 'external')).not.toHaveProperty('connection');
  });

  it('sums up a test: agents, tool calls and how the run ended', () => {
    expect(testLine({ ok: true, branch_id: 'b', status: 'passed', agents: ['a', 'b'], tool_calls: 7, error: null, exit: 'exit 0', stderr_tail: '' })).toBe('2 agents · 7 tool calls · passed');
    expect(testLine({ ok: false, branch_id: 'b', status: 'failed', agents: ['a'], tool_calls: 1, error: 'the driver exited with code 3', exit: 'exit 3', stderr_tail: 'boom' })).toBe('1 agent · 1 tool call · failed');
  });
  it("describes a Code team as its driver, not as 0 agents", async () => {
    const { runLine } = await import('@/model/summary');
    expect(runLine(codeTeam(conn))).toBe('Your code · its driver registers the agents');
    expect(runLine({ id: 'code-c1', name: 'x', agents: [] })).toBe('Your code · its driver registers the agents');
  });
});
