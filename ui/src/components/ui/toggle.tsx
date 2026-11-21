import { cn } from '@/lib/cn';

/** An on/off switch: green when on. The label names it for screen readers and clicks. */
export function Toggle({ checked, onChange, label, description, disabled }: {
  checked: boolean;
  onChange: (on: boolean) => void;
  label: string;
  description?: string;
  disabled?: boolean;
}) {
  return (
    <label className="inline-flex cursor-pointer items-start gap-3">
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        data-on={checked || undefined}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={cn(
          'relative mt-0.5 h-5 w-9 shrink-0 rounded-full border transition-colors disabled:opacity-50',
          checked ? 'border-ok bg-ok' : 'border-line-strong bg-sunken',
        )}
      >
        <span className={cn('absolute top-0.5 size-3.5 rounded-full bg-surface shadow-panel transition-transform', checked ? 'translate-x-4' : 'translate-x-0.5')} />
      </button>
      <span className="grid gap-0.5">
        <span className="text-sm font-medium text-fg">{label}</span>
        {description && <span className="text-xs text-faint">{description}</span>}
      </span>
    </label>
  );
}
