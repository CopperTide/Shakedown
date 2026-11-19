import type { ReactNode } from 'react';
import { cn } from '@/lib/cn';
import { Tag } from './status';

/** "step 12" and its kind as a tag ("tool call"); `children` sit between the two (the agent, say). */
export function StepLabel({ seq, kind, className, children }: { seq: number; kind: string; className?: string; children?: ReactNode }) {
  return (
    <>
      <span className={cn('font-mono font-semibold', className)}>step {seq}</span>
      {children}
      <Tag>{kind.replace('_', ' ')}</Tag>
    </>
  );
}
