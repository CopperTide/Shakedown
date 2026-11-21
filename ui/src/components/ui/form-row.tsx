import type { ReactNode } from 'react';

/** A settings form row: the label and its help on the left, the control on the right. */
export function FormRow({ label, help, children }: { label: string; help?: ReactNode; children: ReactNode }) {
  return (
    <div className="grid gap-7 border-b border-line py-[18px] last:border-b-0 md:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]">
      <div>
        <h4 className="text-sm font-medium text-fg">{label}</h4>
        {help && <p className="mt-1 text-xs leading-relaxed text-faint">{help}</p>}
      </div>
      <div className="flex min-w-0 flex-col gap-3">{children}</div>
    </div>
  );
}
