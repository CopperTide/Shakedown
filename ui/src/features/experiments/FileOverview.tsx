import { AlertTriangle } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Card } from '@/components/ui/card';
import { usd } from '@/lib/format';
import type { ExperimentBody, Problem } from '@/model/project';

/** Setups, repeats, budget and parent of a file experiment, with any problems in its file. */
export default function FileOverview({ body, labels, problems }: {
  body: ExperimentBody;
  labels: Record<string, string>;
  problems: Problem[];
}) {
  const total = body.setups.length * body.repeats.per_setup;
  return (
    <div className="grid gap-4">
      {problems.length > 0 && (
        <Card title="Problems in this experiment's file">
          <ul className="divide-y divide-line">
            {problems.map((p) => (
              <li key={`${p.pointer}${p.message}`} className="flex items-center gap-2 px-[18px] py-2.5 text-sm text-warn">
                <AlertTriangle className="size-4" aria-hidden="true" />{p.message}
              </li>
            ))}
          </ul>
        </Card>
      )}
      <Card title="Setups" description={`${body.repeats.per_setup} runs each${body.repeats.paired_seeds === false ? '' : ', paired seeds'}`} footer={<span>{total} runs</span>}>
        <ul className="divide-y divide-line">
          {body.setups.map((s) => (
            <li key={s} className="px-[18px] py-2.5 text-sm font-medium">{labels[s] ?? s}</li>
          ))}
        </ul>
      </Card>
      <Card title="Budget">
        <p className="px-[18px] py-3 text-sm text-muted">
          {body.budget?.per_run_usd != null ? `${usd(body.budget.per_run_usd)} a run` : 'No cap per run'}
          {body.budget?.per_deployment_usd != null && ` · ${usd(body.budget.per_deployment_usd)} a deployment`}
        </p>
      </Card>
      {body.parent && <p className="text-sm text-muted">Branched from <Link className="text-accent" to={`/experiments/file/${body.parent}`}>{labels[body.parent] ?? body.parent}</Link></p>}
    </div>
  );
}
