import type { ComponentProps } from 'react';
import { cn } from '@/lib/cn';

/** Monospace text on the sunken surface: JSON, a diff, a driver's stderr. Scrolls inside its own box. */
export function CodeBlock({ className, ...props }: ComponentProps<'pre'>) {
  return <pre className={cn('overflow-auto rounded-md border border-line bg-sunken p-2 font-mono text-2xs', className)} {...props} />;
}
