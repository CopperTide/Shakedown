// The task of the world's package a scenario plays: picked from the package's tasks, searched and
// grouped; typed in when the world names no package or its tasks can't be read.
import { ChevronDown } from 'lucide-react';
import { type KeyboardEvent, useId, useState } from 'react';
import { worldBenchmark } from '@/api/benchmark';
import { Hint } from '@/components/ui/hint';
import { Input } from '@/components/ui/input';
import { useQuery } from '@/hooks/useQuery';
import { cn } from '@/lib/cn';
import type { TaskSummary } from '@/model/types';
import { TextField } from '../fields';
import { taskGroups } from './tasks';

const LABEL = 'Task';
const HINT = "The task of the world's runtime this scenario plays.";

/** One task as a line: its id, then its reason. */
function TaskLine({ id, reason }: { id: string; reason?: string }) {
  return (
    <span className="flex min-w-0 items-baseline gap-2">
      <span className="shrink-0 font-mono text-xs">{id}</span>
      {reason && <span className="truncate text-muted">{reason}</span>}
    </span>
  );
}

/** The search box and the grouped list; Enter takes the first match, Escape closes. */
function TaskList({ tasks, value, onPick, onClose }: { tasks: TaskSummary[]; value: string; onPick: (id: string) => void; onClose: () => void }) {
  const [query, setQuery] = useState('');
  const groups = taskGroups(tasks, query);
  const keys = (e: KeyboardEvent) => {
    if (e.key === 'Escape') onClose();
    if (e.key === 'Enter' && groups[0]) { e.preventDefault(); onPick(groups[0].tasks[0].id); }
  };
  return (
    <div className="grid gap-1.5 rounded-md border border-line bg-raised p-1.5">
      <Input type="search" autoFocus aria-label="Search tasks" placeholder="Search by id or reason" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={keys} />
      <div role="listbox" aria-label="Tasks" className="max-h-72 overflow-y-auto">
        {groups.map((g) => (
          <div key={g.group ?? ''} role="group" aria-label={g.group ?? 'Tasks'}>
            {g.group && <div className="px-2 pt-1.5 pb-0.5 text-2xs font-medium text-muted">{g.group}</div>}
            {g.tasks.map((t) => (
              <button key={t.id} type="button" role="option" aria-selected={t.id === value} onClick={() => onPick(t.id)}
                className={cn('flex w-full rounded-sm px-2 py-1 text-left text-sm hover:bg-hover', t.id === value && 'bg-hover')}>
                <TaskLine id={t.id} reason={t.reason} />
              </button>
            ))}
          </div>
        ))}
        {!groups.length && <p className="px-2 py-3 text-xs text-muted">No task matches.</p>}
      </div>
    </div>
  );
}

/** The current task on a button that opens the list; a value the package lacks is shown, and said. */
function Picker({ name, tasks, value, onChange }: { name: string; tasks: TaskSummary[]; value: string; onChange: (v: string | undefined) => void }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const current = tasks.find((t) => t.id === value);
  const pick = (id: string) => { onChange(id); setOpen(false); };
  return (
    <div className="grid gap-1 text-sm">
      <span id={`${id}-label`} className="text-muted"><Hint hint={HINT}>{LABEL}</Hint></span>
      <button type="button" aria-labelledby={`${id}-label ${id}-value`} aria-expanded={open} onClick={() => setOpen((o) => !o)}
        className="flex h-7 min-w-0 items-center gap-2 rounded-md border border-line bg-raised px-2 text-left hover:border-line-strong">
        <span id={`${id}-value`} className="min-w-0 flex-1">{value ? <TaskLine id={value} reason={current?.reason} /> : <span className="text-faint">Choose a task</span>}</span>
        <ChevronDown aria-hidden="true" className="size-3.5 shrink-0 text-muted" />
      </button>
      {value && !current && <span className="text-xs text-tone-warn">{`Not a task of ${name}.`}</span>}
      {open && <TaskList tasks={tasks} value={value} onPick={pick} onClose={() => setOpen(false)} />}
    </div>
  );
}

/** The task picker of the project world `world`; free text while its tasks load or when they can't be read. */
export default function TaskPicker({ world, value, onChange }: { world: string; value: string; onChange: (v: string | undefined) => void }) {
  const bench = useQuery(world ? `benchmark:world:${world}` : null, () => worldBenchmark(world));
  if (bench.data) return <Picker name={bench.data.name} tasks={bench.data.tasks} value={value} onChange={onChange} />;
  return (
    <div className="grid gap-1">
      <TextField label={LABEL} hint={HINT} mono placeholder="t1" value={value} onChange={(v) => onChange(v || undefined)} />
      {bench.error && <span className="text-xs text-muted"><Hint hint="Type the task's id.">{`Can't list the world's tasks (${bench.error})`}</Hint></span>}
    </div>
  );
}
