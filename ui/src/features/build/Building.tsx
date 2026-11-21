import { Play } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Panel } from '@/components/ui/panel';
import type { BuiltWorld } from '@/model/world';

const seconds = (ms = 0) => `${Math.round(ms / 1000)} s`;

interface Props {
  world: BuiltWorld; onRun: () => void; onExperiment: () => void; onRetry: () => void;
  /** Try again under way, or why it was refused. */
  retry?: { busy: boolean; error: string | null };
}

/**
 * A world being prepared (its name, the time so far, an indeterminate bar), then ready with Run and New experiment, or
 * failed (or never prepared on this machine) with why and Try again.
 */
export function BuildingView({ world, onRun, onExperiment, onRetry, retry }: Props) {
  const name = <span className="text-xl font-semibold tracking-tight">{world.title}</span>;
  if (world.state === 'ready') {
    return (
      <Panel className="flex flex-wrap items-center gap-3 p-4">
        {name}<span role="status" className="text-sm">{`✓ Ready · ${seconds(world.elapsed_ms)}`}</span>
        <div className="ml-auto flex gap-2"><Button onClick={onExperiment}>New experiment</Button><Button variant="primary" className="h-9 px-5" onClick={onRun}><Play />Run</Button></div>
      </Panel>
    );
  }
  if (world.state === 'failed' || world.state === 'not_prepared') {
    const failed = world.state === 'failed';
    return (
      <Panel className="flex flex-col gap-3 p-4">
        <div className="flex flex-wrap items-baseline gap-3">{name}<span role="status" className="text-sm font-medium text-alarm">{failed ? '✕ Failed' : '✕ Not prepared'}</span></div>
        <p className="text-sm text-alarm">{world.error ?? (failed ? '' : 'Its base is not prepared on this machine.')}</p>
        {retry?.error && <p className="text-sm text-alarm">{retry.error}</p>}
        <Button className="self-start" disabled={retry?.busy} onClick={onRetry}>Try again</Button>
      </Panel>
    );
  }
  return (
    <Panel className="flex flex-col gap-4 p-4">
      <div className="flex flex-wrap items-baseline gap-3">{name}<span role="status" className="text-sm text-muted">{`◐ Preparing · ${seconds(world.elapsed_ms)}`}</span></div>
      <span aria-label="Preparing" className="relative block h-1 w-full overflow-hidden rounded-full bg-sunken"><span className="absolute inset-y-0 w-1/3 animate-slide rounded-full bg-line-strong" /></span>
    </Panel>
  );
}
