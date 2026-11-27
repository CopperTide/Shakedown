// A JSON data file as tables, where it is records: a list of objects; an object of objects keyed
// by id (`{"u1": {...}}`); or an object of such sections (a database: `{"users": {...}, "flights": [...]}`).
// Every row and cell carries its JSON pointer, so one value can be set in place.

/** One row: its key (an id, or an index) and its record. */
export interface Row { key: string; pointer: string; record: Record<string, unknown> }

/** One table: its title (the section, or the file), its columns and rows. */
export interface Table { title: string; pointer: string; columns: string[]; rows: Row[] }

/** Most rows whose keys make a table's columns. */
const SAMPLE = 200;

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** A JSON pointer segment (RFC 6901). */
export const segment = (s: string) => s.replace(/~/g, '~0').replace(/\//g, '~1');

/** `v` as rows, when it is a list of records or a mapping of records. */
function rowsOf(v: unknown, at: string): Row[] | null {
  if (Array.isArray(v) && v.length > 0 && v.every(isRecord)) {
    return v.map((record, i) => ({ key: String(i), pointer: `${at}/${i}`, record }));
  }
  if (isRecord(v)) {
    const entries = Object.entries(v);
    if (entries.length > 0 && entries.every(([, r]) => isRecord(r))) {
      return entries.map(([key, record]) => ({ key, pointer: `${at}/${segment(key)}`, record: record as Record<string, unknown> }));
    }
  }
  return null;
}

/** Whether column `k` holds nested values (objects, lists) in the sample rows. */
const nested = (rows: Row[], k: string) => rows.some((r) => typeof r.record[k] === 'object' && r.record[k] !== null);

/** A table of `rows`: every key in the sample is a column, plain values first, then nested ones, each in first-seen order. */
function table(title: string, pointer: string, rows: Row[]): Table {
  const sample = rows.slice(0, SAMPLE);
  const seen: string[] = [];
  for (const r of sample) for (const k of Object.keys(r.record)) if (!seen.includes(k)) seen.push(k);
  const columns = [...seen.filter((k) => !nested(sample, k)), ...seen.filter((k) => nested(sample, k))];
  return { title, pointer, columns, rows };
}

/** The tables of `doc`: one per section of a database, else one for the file; none when it is not records. */
export function tablesOf(doc: unknown, title: string): Table[] {
  if (isRecord(doc)) {
    const sections = Object.entries(doc).map(([k, v]) => [k, rowsOf(v, `/${segment(k)}`)] as const);
    // Every value records: a database of sections (an object of plain records is one keyed table below).
    if (sections.length > 0 && sections.every(([, r]) => r !== null)) {
      return sections.map(([k, r]) => table(k, `/${segment(k)}`, r!));
    }
  }
  const rows = rowsOf(doc, '');
  return rows ? [table(title, '', rows)] : [];
}

/** A cell as one line: text as it is, anything else as compact JSON. */
export function cellText(v: unknown): string {
  if (v === undefined) return '';
  if (typeof v === 'string') return v;
  return JSON.stringify(v);
}

/** What a typed cell means: JSON when it parses (a number, true, an object), else the text itself. */
export function parsedCell(text: string, was: unknown): unknown {
  if (typeof was === 'string') return text;
  try { return JSON.parse(text); } catch { return text; }
}

/** The rows of `t` whose key or any cell holds `query` (any case). */
export function matching(t: Table, query: string): Row[] {
  const q = query.trim().toLowerCase();
  if (!q) return t.rows;
  return t.rows.filter((r) => r.key.toLowerCase().includes(q) || Object.values(r.record).some((v) => cellText(v).toLowerCase().includes(q)));
}
