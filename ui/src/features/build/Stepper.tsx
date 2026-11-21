import { cn } from '@/lib/cn';
import { STEP_LABEL, STEPS, type Step } from './builder';

/** The six steps in a row: done ones ticked and clickable, the current one marked; on phones, "Step 3 of 6 · Systems". */
export default function Stepper({ step, onStep }: { step: Step; onStep: (s: Step) => void }) {
  const at = STEPS.indexOf(step);
  return (
    <>
      <nav aria-label="Steps" className="hidden gap-1 overflow-x-auto rounded-md bg-sunken p-1 sm:flex">
        {STEPS.map((s, i) => {
          const done = i < at;
          const dot = <span aria-hidden="true" className={cn('grid size-[18px] place-items-center rounded-full border text-2xs',
            done ? 'border-transparent bg-fg text-bg' : i === at ? 'border-accent text-accent' : 'border-line-strong')}>{done ? '✓' : i + 1}</span>;
          const cls = cn('flex h-8 min-w-max flex-1 items-center gap-2 rounded-[4px] px-3 text-sm whitespace-nowrap',
            i === at ? 'bg-raised font-medium text-fg shadow-panel' : 'text-muted', done && 'hover:text-fg');
          return done
            ? <button key={s} type="button" className={cls} onClick={() => onStep(s)}>{dot}{STEP_LABEL[s]}</button>
            : <span key={s} className={cls} aria-current={i === at ? 'step' : undefined}>{dot}{STEP_LABEL[s]}</span>;
        })}
      </nav>
      <div className="text-sm text-muted sm:hidden">{`Step ${at + 1} of ${STEPS.length} · ${STEP_LABEL[step]}`}</div>
    </>
  );
}
