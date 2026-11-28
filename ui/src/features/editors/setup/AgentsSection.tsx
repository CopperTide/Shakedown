// Built-in agents: each one's role, model and the key-store name of its model key. One form adds an
// agent, or edits the one picked from the list in place.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { EmptyState } from '@/components/ui/state';
import type { BuiltinAgent, SetupBody } from '@/model/project';
import { addTo, removeAt, replaceAt } from '../edit';
import { ItemRows, SectionHead, TextField } from '../fields';

const EMPTY = { id: '', role: '', model: '', key: '', tools: '*' };
type AgentForm = typeof EMPTY;

/** The agent the form describes; tools are comma-separated, `*` for all. */
export function agentOf(f: AgentForm): BuiltinAgent {
  const tools = f.tools.split(',').map((t) => t.trim()).filter(Boolean);
  return { id: f.id, role: f.role, model: f.model, key: f.key, tools: tools.length ? tools : ['*'] };
}

/** The form an agent fills, to edit it. */
export const formOf = (a: BuiltinAgent): AgentForm => ({ id: a.id, role: a.role, model: a.model, key: a.key, tools: (a.tools ?? ['*']).join(', ') });

/** The form's id is another agent's (not the one being edited): ids are unique in a team. */
export const idTaken = (f: AgentForm, agents: BuiltinAgent[], editing: number | null) =>
  agents.some((a, i) => i !== editing && a.id === f.id);

/** Where the row being edited is once row `removed` is gone: null when it was that row. */
const afterRemove = (editing: number | null, removed: number) =>
  editing == null || editing === removed ? null : editing > removed ? editing - 1 : editing;

/** The add-or-edit form: `editing` names the agent being edited, null when adding one. */
function AgentFormCard({ f, setF, editing, taken, onCancel, onSubmit }: {
  f: AgentForm; setF: (f: AgentForm) => void; editing: string | null; taken: boolean; onCancel: () => void; onSubmit: () => void;
}) {
  const field = (k: keyof AgentForm) => ({ value: f[k], onChange: (v: string) => setF({ ...f, [k]: v }) });
  return (
    <Card title={editing == null ? 'Add an agent' : `Edit ${editing}`}>
      <div className="grid gap-3 p-[18px] md:grid-cols-2">
        <TextField label="Agent id" mono {...field('id')} />
        <TextField label="Model" mono placeholder="claude-sonnet" {...field('model')} />
        <TextField label="Role prompt" multiline {...field('role')} />
        <TextField label="Model key name" mono hint="A name from the key store, never the key itself." placeholder="anthropic-key" {...field('key')} />
        <TextField label="Tools" mono hint="* for every tool, or names separated by commas." placeholder="*" {...field('tools')} />
      </div>
      <div className="flex items-center justify-end gap-2 border-t border-line px-[18px] py-2.5">
        {taken && <p role="alert" className="mr-auto text-sm text-alarm">Another agent is already called {f.id}.</p>}
        {editing != null && <Button variant="ghost" onClick={onCancel}>Cancel</Button>}
        <Button disabled={!f.id || !f.model || !f.key || taken} onClick={onSubmit}>{editing == null ? 'Add agent' : 'Save agent'}</Button>
      </div>
    </Card>
  );
}

export default function AgentsSection({ setup, onChange }: { setup: SetupBody; onChange: (s: SetupBody) => void }) {
  const [f, setF] = useState(EMPTY);
  const [editing, setEditing] = useState<number | null>(null);
  if (setup.team.kind !== 'builtin') return <div className="grid gap-4"><SectionHead title="Agents" /><EmptyState title="Nothing to set" hint="This team brings its own agents." /></div>;
  const team = setup.team;
  const setAgents = (agents: BuiltinAgent[]) => onChange({ ...setup, team: { ...team, agents } });
  const done = () => { setF(EMPTY); setEditing(null); };
  const submit = () => {
    setAgents(editing == null ? addTo(team.agents, agentOf(f)) : replaceAt(team.agents, editing, { ...team.agents[editing], ...agentOf(f) }));
    done();
  };
  const remove = (i: number) => {
    setAgents(removeAt(team.agents, i));
    const next = afterRemove(editing, i);
    if (next == null) setF(EMPTY);
    setEditing(next);
  };
  return (
    <div className="grid gap-4">
      <SectionHead title="Agents" about="Each agent's role and model. Keys are names from the key store; the key itself never goes in a file." />
      <Card>
        <ItemRows items={team.agents} empty="No agents yet." onRemove={remove} onEdit={(i) => { setF(formOf(team.agents[i])); setEditing(i); }} editing={editing}
          render={(a) => <span><span className="font-medium">{a.id}</span> <span className="text-faint">· {a.model} · key {a.key}</span></span>} />
      </Card>
      <AgentFormCard f={f} setF={setF} editing={editing == null ? null : team.agents[editing].id} taken={idTaken(f, team.agents, editing)} onCancel={done} onSubmit={submit} />
    </div>
  );
}
