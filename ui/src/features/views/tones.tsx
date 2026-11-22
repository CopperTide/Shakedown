// The renderer's palette for a view's tones: a view file names a tone, never a colour.
import { cn } from '@/lib/cn';
import type { Tone } from './types';

export const TONE_TEXT: Record<Tone, string> = {
  neutral: 'text-fg', active: 'text-tone-active', done: 'text-tone-done', warn: 'text-tone-warn', critical: 'text-tone-critical', accent: 'text-tone-accent',
};

export const TONE_BG: Record<Tone, string> = {
  neutral: 'bg-tone-neutral', active: 'bg-tone-active', done: 'bg-tone-done', warn: 'bg-tone-warn', critical: 'bg-tone-critical', accent: 'bg-tone-accent',
};

export const TONE_SOFT: Record<Tone, string> = {
  neutral: 'bg-sunken text-muted', active: 'bg-tone-active/12 text-tone-active', done: 'bg-tone-done/14 text-tone-done',
  warn: 'bg-tone-warn/14 text-tone-warn', critical: 'bg-tone-critical/12 text-tone-critical', accent: 'bg-tone-accent/12 text-tone-accent',
};

export const TONE_RULE: Record<Tone, string> = {
  neutral: 'border-l-line-strong', active: 'border-l-tone-active', done: 'border-l-tone-done', warn: 'border-l-tone-warn', critical: 'border-l-tone-critical', accent: 'border-l-tone-accent',
};

/** The CSS variable a canvas reads for a tone. */
export const toneVar = (t: Tone) => `--tone-${t}`;

/** A status chip: the tone's ink on its tint, square-cut, mono. */
export function Chip({ tone, children, className, title }: { tone: Tone; children: string; className?: string; title?: string }) {
  return (
    <span title={title} className={cn('inline-block max-w-full truncate rounded-[2px] px-1 align-middle font-mono text-[10.5px] leading-[15px] font-medium tabular-nums', TONE_SOFT[tone], className)}>
      {children}
    </span>
  );
}
