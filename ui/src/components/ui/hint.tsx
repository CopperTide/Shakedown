// Where extra explanation lives: on the title or label itself. Hovering it (desktop) or tapping it
// (phone) shows the words; there is no icon, so screens show names and data only.
import * as PopoverPrimitive from '@radix-ui/react-popover';
import { type ReactNode, useState } from 'react';
import { cn } from '@/lib/cn';

export function Hint({ hint, children, className }: { hint?: ReactNode; children: ReactNode; className?: string }) {
  const [open, setOpen] = useState(false);
  if (!hint) return <>{children}</>;
  return (
    <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
      <PopoverPrimitive.Anchor asChild>
        <span className={cn('cursor-help', className)}
          onPointerEnter={(e) => { if (e.pointerType === 'mouse') setOpen(true); }}
          onPointerLeave={(e) => { if (e.pointerType === 'mouse') setOpen(false); }}
          onPointerUp={(e) => { if (e.pointerType !== 'mouse') { e.preventDefault(); setOpen((o) => !o); } }}>
          {children}
        </span>
      </PopoverPrimitive.Anchor>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Content side="top" align="start" sideOffset={4} onOpenAutoFocus={(e) => e.preventDefault()}
          className="z-50 max-w-xs rounded-md border border-line bg-raised px-2.5 py-1.5 text-xs font-normal whitespace-pre-line text-fg shadow-pop">
          {hint}
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  );
}
