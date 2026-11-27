// What the run's pages share around their content: the errors above it and the step panel beside it.
import { Maximize2, Minimize2 } from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
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

/** Whether the step panel has the page's whole width; Escape gives the width back. */
export function useExpanded(): [boolean, (on: boolean) => void] {
  const [expanded, setExpanded] = useState(false);
  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setExpanded(false); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [expanded]);
  return [expanded, setExpanded];
}

/** Full width and back: the step panel's own toggle. */
function ExpandButton({ expanded, onExpand }: { expanded: boolean; onExpand: (on: boolean) => void }) {
  return (
    <Button variant="ghost" size="sm" onClick={() => onExpand(!expanded)} title={expanded ? 'Back beside the timeline (Esc)' : 'Show the step across the whole page'}>
      {expanded ? <><Minimize2 />Side panel</> : <><Maximize2 />Full width</>}
    </Button>
  );
}

/**
 * The side panel that follows the page down on wide screens: the selected step, in full. `nav`
 * (previous and next step) sits in its header; with `onExpand`, it can take the page's whole width and give it back.
 */
export function StepSidePanel({ children, nav, expanded = false, onExpand }: { children: ReactNode; nav?: ReactNode; expanded?: boolean; onExpand?: (on: boolean) => void }) {
  return (
    <Panel className="lg:sticky lg:top-16 lg:max-h-[calc(100vh-88px)] lg:overflow-auto">
      <PanelHeader title="Step">{nav}{onExpand && <ExpandButton expanded={expanded} onExpand={onExpand} />}</PanelHeader>
      {children}
    </Panel>
  );
}
