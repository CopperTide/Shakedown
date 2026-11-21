// What the run's pages share around their content: the errors above it and the step panel beside it.
import type { ReactNode } from 'react';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { ErrorState } from '@/components/ui/state';
import type { BranchDetail } from '@/model/types';

/** A failed reload of the run (with a retry), and the error the run itself stopped with. */
export function RunErrorBanner({ error, onRetry, detail }: { error: string | null; onRetry: () => void; detail?: BranchDetail }) {
  return (
    <>
      {error && <ErrorState error={error} onRetry={onRetry} />}
      {detail?.error && <ErrorState error={detail.error} hint="The run stopped with this error. Its steps up to here are recorded." />}
    </>
  );
}

/** The side panel that follows the page down on wide screens: the selected step, in full. */
export function StepSidePanel({ children }: { children: ReactNode }) {
  return (
    <Panel className="lg:sticky lg:top-16 lg:max-h-[calc(100vh-88px)] lg:overflow-auto">
      <PanelHeader title="Step" />
      {children}
    </Panel>
  );
}
