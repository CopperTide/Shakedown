// How the setup runs: the kind of team, and the fields that kind needs.
import { Card } from '@/components/ui/card';
import type { SetupBody, TeamSpec } from '@/model/project';
import { cn } from '@/lib/cn';
import { SectionHead, TextField } from '../fields';
import { TEAM_KINDS, teamFor } from './teams';

function KindFields({ team, onChange }: { team: TeamSpec; onChange: (t: TeamSpec) => void }) {
  switch (team.kind) {
    case 'http': return (
      <div className="grid gap-3 md:grid-cols-2">
        <TextField label="Endpoint URL" mono value={team.url} onChange={(v) => onChange({ ...team, url: v })} placeholder="https://…" />
        <TextField label="Key-store name" mono value={team.auth?.key ?? ''} onChange={(v) => onChange({ ...team, auth: v ? { key: v } : undefined })} placeholder="A name from the key store, never the key" />
      </div>);
    case 'code': return <TextField label="Command" mono value={team.command} onChange={(v) => onChange({ ...team, command: v })} placeholder="uv run python crew.py" />;
    case 'madl': return <TextField label="MADL file" mono value={team.file} onChange={(v) => onChange({ ...team, file: v })} />;
    default: return null;
  }
}

export default function TeamKindSection({ setup, onChange }: { setup: SetupBody; onChange: (s: SetupBody) => void }) {
  return (
    <div className="grid gap-4">
      <SectionHead title="How it runs" about="Shakedown's built-in agents, or your own. Every setup in an experiment gets the same task, so results compare fairly." />
      <div role="radiogroup" aria-label="Kind of team" className="grid gap-3 md:grid-cols-3">
        {TEAM_KINDS.map((k) => (
          <button key={k.kind} type="button" role="radio" aria-checked={setup.team.kind === k.kind} aria-label={k.label}
            onClick={() => setup.team.kind !== k.kind && onChange({ ...setup, team: teamFor(k.kind) })}
            className={cn('rounded-lg border bg-surface p-4 text-left hover:border-line-strong', setup.team.kind === k.kind ? 'border-fg' : 'border-line')}>
            <div className="text-sm font-semibold">{k.label}</div><div className="mt-1 text-xs text-faint">{k.about}</div>
          </button>
        ))}
      </div>
      <Card><div className="p-[18px]"><KindFields team={setup.team} onChange={(t) => onChange({ ...setup, team: t })} /></div></Card>
    </div>
  );
}
