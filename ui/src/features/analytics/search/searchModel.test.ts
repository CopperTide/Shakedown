// Step search's pure pieces: the address round trip, the API query, snippets split at their
// marks, hits grouped by run, and where a hit opens.
import { describe, expect, it } from 'vitest';
import type { SearchHit } from '@/api/analytics';
import { byRun, HIT_END, HIT_START, NO_SEARCH, parseSearch, searchHref, searchQuery, snippetParts, stepHref, toggled, writeSearch } from './searchModel';

const hit = (run: string, seq: number, over: Partial<SearchHit> = {}): SearchHit => ({
  run_id: run, run_name: `Run ${run}`, branch_id: `b-${run}`, seq, kind: 'reasoning', agent: 'ops', at: '2026-11-25T10:00:00Z', snippet: '', ...over,
});

describe('step search', () => {
  it('keeps the words and picks in the address beside other keys', () => {
    const sp = writeSearch(new URLSearchParams('tab=search&range=7d'), { q: 'crew swap', kind: ['reasoning', 'tool_call'], agent: ['ops'] });
    expect(sp.get('tab')).toBe('search');
    expect(sp.get('range')).toBe('7d');
    expect(parseSearch(sp)).toEqual({ q: 'crew swap', kind: ['reasoning', 'tool_call'], agent: ['ops'] });
    const cleared = writeSearch(sp, NO_SEARCH);
    expect([...cleared.keys()].sort()).toEqual(['range', 'tab']);
  });

  it('asks the API only with words, each list a JSON array, from an offset', () => {
    expect(searchQuery({ ...NO_SEARCH, q: '   ' })).toBeNull();
    const q = new URLSearchParams(searchQuery({ q: ' crew ', kind: ['tool_call'], agent: ['a,b'] }, 50)!.slice(1));
    expect(q.get('q')).toBe('crew');
    expect(JSON.parse(q.get('kind')!)).toEqual(['tool_call']);
    expect(JSON.parse(q.get('agent')!)).toEqual(['a,b']);
    expect(q.get('offset')).toBe('50');
    expect(searchQuery({ ...NO_SEARCH, q: 'x' })).toBe('?q=x');
  });

  it('splits a snippet at its marked matches', () => {
    const s = `The ${HIT_START}crews${HIT_END} are short … swap a ${HIT_START}crew${HIT_END}`;
    expect(snippetParts(s)).toEqual([
      { text: 'The ', hit: false }, { text: 'crews', hit: true }, { text: ' are short … swap a ', hit: false }, { text: 'crew', hit: true },
    ]);
    expect(snippetParts('no match')).toEqual([{ text: 'no match', hit: false }]);
    expect(snippetParts(`${HIT_START}open`)).toEqual([{ text: 'open', hit: true }]);
    expect(snippetParts('')).toEqual([]);
  });

  it('groups hits by run in the order the runs first came, hits kept in order', () => {
    const groups = byRun([hit('a', 9), hit('b', 4), hit('a', 3)]);
    expect(groups.map((g) => [g.runId, g.runName, g.hits.map((h) => h.seq)])).toEqual([['a', 'Run a', [9, 3]], ['b', 'Run b', [4]]]);
  });

  it('opens the run page at the step, and the palette opens the Search tab', () => {
    expect(stepHref(hit('r 1', 12))).toBe('/runs/r%201/b-r%201?step=12');
    expect(searchHref('crew swap')).toBe('/analytics?tab=search&q=crew+swap');
    expect(toggled(['a'], 'b')).toEqual(['a', 'b']);
    expect(toggled(['a', 'b'], 'a')).toEqual(['b']);
  });
});
