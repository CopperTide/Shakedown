// The Analytics page's Search tab: find any step across every run by its words (reasoning,
// messages, tool calls, errors), narrow by kind and agent, and open the run page at that step.
// The search lives in the address, so a search can be shared.
import { Search, X } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { type FacetCount, type SearchHit, type SearchResults, searchSteps } from '@/api/analytics';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Panel } from '@/components/ui/panel';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/state';
import { StepLabel } from '@/components/ui/step-label';
import { useQuery } from '@/hooks/useQuery';
import { cn } from '@/lib/cn';
import { ago, formatTime } from '@/lib/time';
import { path } from '@/lib/url';
import { byRun, kindLabel, parseSearch, type RunGroup, type StepSearch, searchQuery, snippetParts, stepHref, toggled, writeSearch } from './searchModel';

/** The search, kept in the address. */
function useStepSearch() {
  const [sp, setSp] = useSearchParams();
  const key = ['q', 'kind', 'agent'].map((k) => sp.getAll(k).join('\n')).join('\t');
  const search = useMemo(() => parseSearch(sp), [key]); // eslint-disable-line react-hooks/exhaustive-deps
  const setSearch = useCallback((s: StepSearch) => setSp((prev) => writeSearch(prev, s), { replace: true }), [setSp]);
  return { search, setSearch };
}

/** The first page for `s`, and the pages "Show more" added since (dropped when the search changes). */
function useResults(s: StepSearch) {
  const first = searchQuery(s);
  const page = useQuery(first ? `search:${first}` : null, () => searchSteps(first ?? ''), { keepPrevious: true });
  const [more, setMore] = useState<{ key: string | null; hits: SearchHit[]; next: number | null; error: string | null }>({ key: null, hits: [], next: null, error: null });
  const mine = more.key === first ? more : null;
  const next = mine ? mine.next : page.data?.next_offset ?? null;
  const loadMore = async () => {
    const q = next == null ? null : searchQuery(s, next);
    if (!q) return;
    try {
      const r = await searchSteps(q);
      setMore({ key: first, hits: [...(mine?.hits ?? []), ...r.hits], next: r.next_offset, error: null });
    } catch (e) {
      setMore({ key: first, hits: mine?.hits ?? [], next, error: e instanceof Error ? e.message : String(e) });
    }
  };
  const hits = page.data ? [...page.data.hits, ...(mine?.hits ?? [])] : [];
  return { page, hits, next, loadMore, moreError: mine?.error ?? null };
}

/** The words box: the address follows a moment after typing stops. */
function SearchBox({ q, onChange }: { q: string; onChange: (q: string) => void }) {
  const [text, setText] = useState(q);
  useEffect(() => { setText(q); }, [q]);
  useEffect(() => {
    if (text === q) return undefined;
    const t = window.setTimeout(() => onChange(text), 250);
    return () => window.clearTimeout(t);
  }, [text]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className="relative min-w-48 flex-1 sm:max-w-md">
      <Search aria-hidden="true" className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-faint" />
      <Input aria-label="Search every run's steps" placeholder="Search reasoning, messages, tool calls…" value={text} autoFocus
        className="h-9 pl-8 pr-8" onChange={(e) => setText(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') onChange(text); }} />
      {text && <button type="button" aria-label="Clear the search" onClick={() => { setText(''); onChange(''); }} className="absolute right-1 top-1.5 rounded p-1 text-muted hover:bg-hover"><X className="size-4" /></button>}
    </div>
  );
}

/** Toggle chips for one facet, each with its match count; picked ones stay listed even at zero. */
function Chips({ label, facets, picked, name, onToggle }: {
  label: string; facets: FacetCount[]; picked: string[]; name: (v: string) => string; onToggle: (v: string) => void;
}) {
  const listed = [...facets, ...picked.filter((p) => !facets.some((f) => f.name === p)).map((p) => ({ name: p, count: 0 }))];
  if (!listed.length) return null;
  return (
    <div role="group" aria-label={label} className="flex flex-wrap items-center gap-1.5">
      <span className="w-12 shrink-0 text-2xs font-medium text-muted">{label}</span>
      {listed.map((f) => {
        const on = picked.includes(f.name);
        return (
          <button key={f.name} type="button" aria-pressed={on} onClick={() => onToggle(f.name)}
            className={cn('flex h-7 items-center gap-1.5 rounded-md border px-2 text-xs', on ? 'border-accent/40 bg-accent-soft text-fg' : 'border-line bg-surface text-muted hover:border-line-strong hover:text-fg')}>
            {name(f.name)}<span className="tabular-nums text-faint">{f.count.toLocaleString()}</span>
          </button>
        );
      })}
    </div>
  );
}

/** A snippet with its matches marked. */
function Snippet({ text }: { text: string }) {
  return (
    <p className="line-clamp-3 break-words text-sm text-muted">
      {snippetParts(text).map((p, i) => (p.hit ? <mark key={i} className="rounded-sm bg-accent-soft text-fg">{p.text}</mark> : <span key={i}>{p.text}</span>))}
    </p>
  );
}

/** One run's hits, each opening the run page at its step. */
function RunHits({ g }: { g: RunGroup }) {
  const first = g.hits[0];
  return (
    <Panel>
      <header className="flex items-baseline gap-2 border-b border-line px-4 py-2.5">
        <h3 className="min-w-0 flex-1 truncate font-display text-lg" title={g.runName}>{g.runName}</h3>
        <span className="text-xs tabular-nums text-muted">{g.hits.length} step{g.hits.length === 1 ? '' : 's'}</span>
        <Link to={path`/runs/${g.runId}/${first.branch_id}`} className="text-xs text-accent hover:underline">Open run</Link>
      </header>
      <ul className="divide-y divide-line">
        {g.hits.map((h) => (
          <li key={`${h.branch_id}:${h.seq}`}>
            <Link to={stepHref(h)} className="flex flex-col gap-1 px-4 py-2.5 hover:bg-hover focus-visible:bg-hover">
              <span className="flex flex-wrap items-center gap-2 text-xs">
                <StepLabel seq={h.seq} kind={kindLabel(h.kind).toLowerCase()}>{h.agent && <span className="text-muted">{h.agent}</span>}</StepLabel>
                <time dateTime={h.at} title={formatTime(h.at)} className="ml-auto text-2xs text-faint">{ago(h.at)}</time>
              </span>
              <Snippet text={h.snippet} />
            </Link>
          </li>
        ))}
      </ul>
    </Panel>
  );
}

function Count({ r, shown }: { r: SearchResults; shown: number }) {
  const total = `${r.total.toLocaleString()}${r.total_capped ? '+' : ''}`;
  return (
    <p role="status" className="text-xs text-muted">
      <span className="font-medium text-fg">{total}</span> matching step{r.total === 1 ? '' : 's'}
      {shown < r.total && `, newest ${shown.toLocaleString()} shown`}
    </p>
  );
}

function Results({ s, setSearch }: { s: StepSearch; setSearch: (s: StepSearch) => void }) {
  const { page, hits, next, loadMore, moreError } = useResults(s);
  const [busy, setBusy] = useState(false);
  const groups = useMemo(() => byRun(hits), [hits]);
  const r = page.data;
  if (!r) return page.error ? <ErrorState error={page.error} onRetry={page.retry} /> : <Panel><LoadingState label="Searching…" rows={3} /></Panel>;
  return (
    <>
      <div className="flex flex-col gap-2">
        <Chips label="Kind" facets={r.kinds} picked={s.kind} name={kindLabel} onToggle={(k) => setSearch({ ...s, kind: toggled(s.kind, k) })} />
        <Chips label="Agent" facets={r.agents} picked={s.agent} name={(a) => a} onToggle={(a) => setSearch({ ...s, agent: toggled(s.agent, a) })} />
      </div>
      {page.error && <ErrorState error={page.error} onRetry={page.reload} />}
      <Count r={r} shown={hits.length} />
      {!hits.length && <Panel><EmptyState title="No step matches" hint="Try fewer or other words, or remove a kind or agent filter." /></Panel>}
      {groups.map((g) => <RunHits key={g.runId} g={g} />)}
      {moreError && <ErrorState error={moreError} />}
      {next != null && (
        <Button className="self-center" disabled={busy} onClick={async () => { setBusy(true); await loadMore(); setBusy(false); }}>
          {busy ? 'Loading…' : 'Show more'}
        </Button>
      )}
    </>
  );
}

export default function SearchTab() {
  const { search, setSearch } = useStepSearch();
  return (
    <div className="flex flex-col gap-4">
      <SearchBox q={search.q} onChange={(q) => setSearch({ ...search, q })} />
      {search.q.trim()
        ? <Results s={search} setSearch={setSearch} />
        : <Panel><EmptyState title="Find any step in any run" hint={<>Words match their other forms (crew finds crews). Use quotes for a phrase, <code>or</code> for either word and <code>-word</code> to leave a word out.</>} /></Panel>}
    </div>
  );
}
