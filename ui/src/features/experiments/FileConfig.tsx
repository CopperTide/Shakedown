// A file experiment's problems, shown first, and its configuration, folded under its runs: a line
// when closed; open, its parent and the editor for its choices.
import { AlertTriangle } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Card } from '@/components/ui/card';
import ExperimentSettings from '@/features/editors/experiment/ExperimentSettings';
import { usd } from '@/lib/format';
import { plural } from '@/lib/plural';
import type { ExperimentBody, Problem } from '@/model/project';

/** "3 setups × 5 runs = 15 runs · paired seeds · $1.50 cap a run". */
export function configLine(body: ExperimentBody): string {
  const per = body.repeats.per_setup;
  const parts = [`${plural(body.setups.length, 'setup')} × ${plural(per, 'run')} = ${plural(body.setups.length * per, 'run')}`];
  if (body.repeats.paired_seeds !== false) parts.push('paired seeds');
  parts.push(body.budget?.per_run_usd != null ? `${usd(body.budget.per_run_usd)} cap a run` : 'no cap per run');
  if (body.budget?.per_deployment_usd != null) parts.push(`${usd(body.budget.per_deployment_usd)} a deployment`);
  return parts.join(' · ');
}

export function FileProblems({ problems }: { problems: Problem[] }) {
  if (!problems.length) return null;
  return (
    <Card title="Problems">
      <ul className="divide-y divide-line">
        {problems.map((p) => (
          <li key={`${p.pointer}${p.message}`} className="flex items-center gap-2 px-[18px] py-2.5 text-sm text-warn">
            <AlertTriangle className="size-4" aria-hidden="true" />{p.message}
          </li>
        ))}
      </ul>
    </Card>
  );
}

export default function FileConfig({ id, body, revision, labels, onSaved }: {
  id: string;
  body: ExperimentBody;
  revision?: number;
  labels: Record<string, string>;
  onSaved: () => void;
}) {
  return (
    <details className="rounded-lg border border-line bg-surface">
      <summary className="cursor-pointer px-4 py-3 text-sm">
        <span className="font-semibold">Configuration</span>
        <span className="ml-2 text-xs text-muted">{configLine(body)}</span>
      </summary>
      <div className="flex flex-col gap-3 border-t border-line p-4">
        {body.parent && <p className="text-sm text-muted">Branched from <Link className="text-accent" to={`/experiments/file/${body.parent}`}>{labels[body.parent] ?? body.parent}</Link></p>}
        <ExperimentSettings id={id} body={body} revision={revision} onSaved={onSaved} />
      </div>
    </details>
  );
}
