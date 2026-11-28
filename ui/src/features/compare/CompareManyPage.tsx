// Three or more runs side by side (`/compare?branches=a,b,c`): what each achieved, and how each
// differs from the first, composed from the pairwise compare. Two go to the pairwise page.
import { useEffect } from 'react';
import { Link, Navigate, useSearchParams } from 'react-router-dom';
import { branch as fetchBranch } from '@/api/branches';
import { compare } from '@/api/compare';
import { PageBody, PageHeader } from '@/components/ui/page';
import { Hint } from '@/components/ui/hint';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import { errorMessage } from '@/lib/errors';
import { track } from '@/lib/productAnalytics';
import type { BranchDetail, CompareResponse } from '@/model/types';
import { endView, lastOf } from './ComparePage';
import { againstFirst, againstLine, branchesIn, compareHref, letterOf, sidesOf } from './many';
import { OutcomeTable } from './OutcomePanel';
import type { SideInput } from './outcomes';

interface Loaded {
  sides: SideInput[];
  /** Each branch after the first against the first. */
  pairs: (CompareResponse | { error: string })[];
}

async function loadMany(ids: string[]): Promise<Loaded> {
  const [base, ...rest] = ids;
  const [details, views, pairs] = await Promise.all([
    Promise.all(ids.map((id) => fetchBranch(id, true))),
    Promise.all(ids.map(endView)),
    Promise.all(rest.map((id) => compare(base, id).catch((e: unknown) => ({ error: errorMessage(e) })))),
  ]);
  const figures = sidesOf(details, pairs);
  return { sides: details.map((detail, i) => ({ detail, side: figures[i], view: lastOf(views[i]) })), pairs };
}

function AgainstFirst({ details, pairs }: { details: BranchDetail[]; pairs: Loaded['pairs'] }) {
  const [base, ...rest] = details;
  return (
    <Panel>
      <PanelHeader title={<Hint hint="Where each run first leaves A, and how its final world differs.">Against A</Hint>} />
      <ul className="divide-y divide-line">
        {rest.map((d, i) => (
          <li key={d.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2 text-sm">
            <span className="w-6 font-semibold">{letterOf(i + 1)}</span>
            <span className="min-w-0 flex-1 text-muted">{againstLine(againstFirst(pairs[i]))}</span>
            {!('error' in pairs[i]) && <Link to={compareHref([base.id, d.id])} className="text-xs text-accent hover:underline" aria-label={`A against ${letterOf(i + 1)} in detail`}>Details</Link>}
          </li>
        ))}
      </ul>
    </Panel>
  );
}

function Many({ ids }: { ids: string[] }) {
  const { data, error, retry } = useQuery(`many:${ids.join(',')}`, () => loadMany(ids));
  useEffect(() => track('compare opened', { of: 'runs' }), []);
  return (
    <>
      <PageHeader title={`Compare ${ids.length} runs`} width="broad" />
      <PageBody width="broad">
        {error && <ErrorState error={error} onRetry={retry} />}
        {!data && !error && <LoadingState label={`Reading ${ids.length} runs…`} rows={4} />}
        {data && (
          <>
            <OutcomeTable sides={data.sides} labels={data.sides.map((_, i) => letterOf(i))} />
            <AgainstFirst details={data.sides.map((s) => s.detail)} pairs={data.pairs} />
          </>
        )}
      </PageBody>
    </>
  );
}

export default function CompareManyPage() {
  const ids = branchesIn(useSearchParams()[0].get('branches'));
  if (ids.length === 2) return <Navigate to={compareHref(ids)} replace />;
  if (ids.length < 2) {
    return <><PageHeader title="Compare runs" /><PageBody><EmptyState title="Pick two or more runs" hint="From a run's experiment menu, tick the runs and choose Compare." /></PageBody></>;
  }
  return <Many ids={ids} />;
}
