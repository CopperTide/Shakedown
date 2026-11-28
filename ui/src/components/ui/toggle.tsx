// An on/off switch on Radix's Switch (role, keyboard and state from the library), in our colours:
// green when on, a light knob left when off and right when on.
import * as SwitchPrimitive from '@radix-ui/react-switch';
import { useId } from 'react';
import { cn } from '@/lib/cn';
import { Hint } from './hint';

export function Toggle({ checked, onChange, label, description, disabled }: {
  checked: boolean;
  onChange: (on: boolean) => void;
  label: string;
  description?: string;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className="inline-flex items-start gap-3">
      <SwitchPrimitive.Root id={id} checked={checked} onCheckedChange={onChange} disabled={disabled} aria-label={label}
        className={cn(
          'peer mt-0.5 inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border transition-colors disabled:cursor-not-allowed disabled:opacity-50',
          'border-line-strong bg-sunken data-[state=checked]:border-ok data-[state=checked]:bg-ok',
        )}>
        <SwitchPrimitive.Thumb className="block size-3.5 translate-x-0.5 rounded-full border border-line-strong bg-[#fbf8f3] shadow-panel transition-transform data-[state=checked]:translate-x-4" />
      </SwitchPrimitive.Root>
      <label htmlFor={id} className="cursor-pointer text-sm font-medium text-fg"><Hint hint={description}>{label}</Hint></label>
    </div>
  );
}
