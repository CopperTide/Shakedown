// Where a file experiment sits: its world, then its scenario, then the experiment itself.
import { Fragment } from 'react';
import { Link } from 'react-router-dom';
import type { ExperimentBody } from '@/model/project';

export default function ExperimentPath({ body, labels }: { body: ExperimentBody; labels: Record<string, string> }) {
  const steps = [
    { label: labels[body.world] ?? body.world, to: `/worlds/${body.world}` },
    { label: labels[body.scenario] ?? body.scenario, to: `/scenarios/${body.scenario}` },
  ];
  return (
    <nav aria-label="Where this experiment sits" className="flex flex-wrap items-center gap-1.5">
      {steps.map((s) => (
        <Fragment key={s.to}>
          <Link to={s.to} className="text-muted transition-colors hover:text-fg">{s.label}</Link>
          <span className="text-faint" aria-hidden="true">›</span>
        </Fragment>
      ))}
      <span aria-current="page" className="font-medium text-fg">{body.label}</span>
    </nav>
  );
}
