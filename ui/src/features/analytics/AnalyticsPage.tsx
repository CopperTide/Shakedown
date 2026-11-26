// Analytics: figures across runs. Tabs from the start (Overview now; Search joins it), the filter
// in the address, and the Overview's tiles, charts and leaderboard over one summary.
import { useCallback, useMemo, useState } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { analyticsSummary, type Summary } from '@/api/analytics';
import { Button } from '@/components/ui/button';
import { PageBody, PageHeader } from '@/components/ui/page';
import { Panel } from '@/components/ui/panel';
import { EmptyState, ErrorState, LoadingState, Notice } from '@/components/ui/state';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { pollWhile, useQuery } from '@/hooks/useQuery';
import CostScatter from './CostScatter';
import FilterBar from './FilterBar';
import { type AnalyticsFilter, apiQuery, DEFAULT_RANGE, isNarrowed, parseFilter, runsHref, writeFilter } from './filters';
import Kpis from './Kpis';
import Leaderboard from './Leaderboard';
import { Card } from './parts';
import PassRateChart from './PassRateChart';

/** Below this many runs the page says the charts are still filling in. */
const FEW = 5;

/** The page's filter, kept in the address. */
function useFilter() {
  const [sp, setSp] = useSearchParams();
  const filter = useMemo(() => parseFilter(sp), [sp]);
  const setFilter = useCallback((f: AnalyticsFilter) => setSp((prev) => writeFilter(prev, f), { replace: true }), [setSp]);
  return { filter, setFilter };
}

/** No run counted: why, and what to do. */
function Empty({ s, filter, setFilter }: { s: Summary; filter: AnalyticsFilter; setFilter: (f: AnalyticsFilter) => void }) {
  if (s.pending.unprocessed > 0) return null;
  if (isNarrowed(filter)) {
    return <EmptyState title="No runs match these filters" hint="Remove a filter or pick a longer time range."
      action={<Button onClick={() => setFilter({ ...filter, world: [], scenario: [], setup: [], model: [] })}>Clear filters</Button>} />;
  }
  if (filter.range !== 'all') {
    return <EmptyState title="No finished runs in this time range" hint="Older runs may show under All."
      action={<Button onClick={() => setFilter({ ...filter, range: 'all' })}>Show all time</Button>} />;
  }
  return <EmptyState title="No finished runs yet" hint="Deploy an experiment to see results here: pass rate, cost and time per setup, as its runs finish."
    action={<Button variant="primary" asChild><Link to="/experiments">Go to Experiments</Link></Button>} />;
}

function Overview({ s, filter, setFilter, now }: { s: Summary; filter: AnalyticsFilter; setFilter: (f: AnalyticsFilter) => void; now: Date }) {
  const nav = useNavigate();
  const t = s.totals;
  const waiting = s.pending.unprocessed > 0;
  return (
    <>
      {waiting && <Notice>Working out the figures of {s.pending.unprocessed} finished run{s.pending.unprocessed === 1 ? '' : 's'}; they appear here in a moment.</Notice>}
      {t.runs === 0 ? <Panel><Empty s={s} filter={filter} setFilter={setFilter} /></Panel> : (
        <>
          <Kpis t={t} />
          {t.runs < FEW && <p className="text-xs text-muted">Only {t.runs} run{t.runs === 1 ? '' : 's'} so far: the charts fill in as more experiments run.</p>}
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <PassRateChart summary={s} onGroup={(group) => setFilter({ ...filter, group })} />
            <CostScatter summary={s} onOpen={(g) => nav(runsHref(filter, now, s.group === 'model' ? { model: g } : { setup: g }))} />
          </div>
          <Card title="Leaderboard" hint="Each setup on each scenario. Click a row to see its runs; click a heading to sort.">
            <div className="-mx-4 -mb-3 border-t border-line">
              <Leaderboard rows={s.leaderboard} onOpen={(r) => nav(runsHref(filter, now, { setup: r.setup, scenario: r.scenario }))} />
            </div>
          </Card>
        </>
      )}
      {s.pending.running > 0 && <p className="text-xs text-faint">{s.pending.running} run{s.pending.running === 1 ? ' is' : 's are'} still going; {s.pending.running === 1 ? 'it counts' : 'they count'} once finished.</p>}
    </>
  );
}

export default function AnalyticsPage() {
  const { filter, setFilter } = useFilter();
  const [now] = useState(() => new Date());
  const query = apiQuery(filter, now);
  const { data, error, reload, retry } = useQuery(`analytics:${query}`, () => analyticsSummary(query), {
    ...pollWhile<Summary>(4000, (s) => s.pending.unprocessed > 0 || s.pending.running > 0), keepPrevious: true,
  });
  return (
    <>
      <PageHeader title="Analytics" width="wide" meta={filter.range === DEFAULT_RANGE ? 'The last 30 days, each run counted once (reruns left out).' : 'Each run counted once (reruns left out).'} />
      <PageBody width="wide">
        <Tabs defaultValue="overview">
          <TabsList className="px-0">
            <TabsTrigger value="overview">Overview</TabsTrigger>
          </TabsList>
          <TabsContent value="overview" className="flex flex-col gap-4 pt-4">
            <FilterBar filter={filter} options={data?.options ?? null} onChange={setFilter} />
            {error && data && <ErrorState error={error} onRetry={reload} />}
            {!data && (error ? <ErrorState error={error} onRetry={retry} /> : <Panel><LoadingState label="Loading analytics…" rows={4} /></Panel>)}
            {data && <Overview s={data} filter={filter} setFilter={setFilter} now={now} />}
          </TabsContent>
        </Tabs>
      </PageBody>
    </>
  );
}
