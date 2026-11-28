import type { ReactNode } from 'react';
import { Hint } from './hint';

/** A settings form row: the label on the left (its help on hovering it), the control on the right. */
export function FormRow({ label, help, children }: { label: string; help?: ReactNode; children: ReactNode }) {
  return (
    <div className="grid gap-7 border-b border-line py-[18px] last:border-b-0 md:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]">
      <h4 className="text-sm font-medium text-fg"><Hint hint={help}>{label}</Hint></h4>
      <div className="flex min-w-0 flex-col gap-3">{children}</div>
    </div>
  );
}
