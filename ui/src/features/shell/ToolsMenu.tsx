// Everything that isn't a section of its own: tools, settings, the theme and the core address.
import { MoreHorizontal } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { CORE } from '@/api/core';
import type { Mode } from '@/app/palette';
import { setThemePref, useTheme } from '@/app/theme';
import { nextPref } from '@/app/themeModel';
import { Button } from '@/components/ui/button';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from '@/components/ui/dropdown-menu';
import { Kbd } from '@/components/ui/kbd';
import ReviewToggle from '@/features/review/ReviewToggle';

const THEME_WORD = { system: 'System', light: 'Light', dark: 'Dark' } as const;

/** "⋯": everything that isn't on the first screen. */
export function ToolsMenu({ onPalette }: { onPalette: (start?: Mode) => void }) {
  const nav = useNavigate();
  const { pref } = useTheme();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="text-muted" aria-label="More"><MoreHorizontal /></Button></DropdownMenuTrigger>
      <DropdownMenuContent className="w-64">
        <DropdownMenuLabel>Tools</DropdownMenuLabel>
        <DropdownMenuItem onSelect={() => nav('/run')}>Run once</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => nav('/experiments/new/direct')}>Experiment on a benchmark…</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => nav('/worlds/new')}>Build a world…</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => onPalette({ kind: 'compare-runs', first: null })}>Compare two runs…</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => onPalette({ kind: 'world' })}>World view of a run…</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => nav('/team')}>Team editor</DropdownMenuItem>
        <ReviewToggle />
        <DropdownMenuLabel>Settings</DropdownMenuLabel>
        <DropdownMenuItem onSelect={() => nav('/settings/models')}>Models</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => nav('/settings/tokens')}>API tokens</DropdownMenuItem>
        <DropdownMenuLabel>App</DropdownMenuLabel>
        <DropdownMenuItem onSelect={() => onPalette()}><span className="flex-1">Command palette</span><Kbd>⌘K</Kbd></DropdownMenuItem>
        <DropdownMenuItem onSelect={(e) => { e.preventDefault(); setThemePref(nextPref(pref)); }}>
          <span className="flex-1">Theme</span><span className="text-2xs text-muted">{THEME_WORD[pref]}</span>
        </DropdownMenuItem>
        <DropdownMenuLabel className="font-normal" title="Open the UI with ?core=<url> to use another Shakedown core, or ?core= to go back to this one">
          Shakedown core: {CORE || 'this server'}
        </DropdownMenuLabel>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

