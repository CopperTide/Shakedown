// A JSON data file as tables: a list of records, a mapping of records, a database of sections.
import { describe, expect, it } from 'vitest';
import { cellText, matching, parsedCell, tablesOf } from './jsonTables';

describe('tablesOf', () => {
  it('reads a list of records as one table with every key a column', () => {
    const [t] = tablesOf([{ id: 'a', n: 1 }, { id: 'b', extra: true }], 'rates.json');
    expect(t.columns).toEqual(['id', 'n', 'extra']);
    expect(t.rows[1]).toMatchObject({ key: '1', pointer: '/1' });
  });

  it('reads a database as one table per section, keyed by id, with escaped pointers', () => {
    const db = { users: { 'u/1': { name: 'Ada' } }, flights: [{ number: 'HAT001' }] };
    const tables = tablesOf(db, 'db.json');
    expect(tables.map((t) => t.title)).toEqual(['users', 'flights']);
    expect(tables[0].rows[0]).toMatchObject({ key: 'u/1', pointer: '/users/u~11' });
    expect(tables[1].rows[0].pointer).toBe('/flights/0');
  });

  it('reads a mapping of plain records as one keyed table, and anything else as none', () => {
    const [t] = tablesOf({ u1: { name: 'Ada', age: 36 } }, 'users.json');
    expect(t.rows[0].pointer).toBe('/u1');
    expect(tablesOf({ a: 1 }, 'x.json')).toEqual([]);
    expect(tablesOf('text', 'x.json')).toEqual([]);
  });
});

describe('cells', () => {
  it('shows text as it is and parses what is typed as the old value was', () => {
    expect(cellText({ a: 1 })).toBe('{"a":1}');
    expect(parsedCell('42', 7)).toBe(42);
    expect(parsedCell('42', 'old')).toBe('42');
    expect(parsedCell('not json', 1)).toBe('not json');
  });

  it('finds rows by key or by any cell', () => {
    const [t] = tablesOf([{ city: 'Denver' }, { city: 'Austin' }], 'x');
    expect(matching(t, 'aus').map((r) => r.key)).toEqual(['1']);
    expect(matching(t, '').length).toBe(2);
  });
});
