// Built-in agents: each one's role, model and the key-store name of its model key.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { BuiltinAgent, SetupBody } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ItemRows, SectionHead, TextField } from '../fields';

const EMPTY = { id: '', role: '', model: '', key: '', tools: '*' };

/** The agent the form describes; tools are comma-separated, `*` for all. */
export function agentOf(f: typeof EMPTY): BuiltinAgent {
  const tools = f.tools.split(',').map((t) => t.trim()).filter(Boolean);
  return { id: f.id, role: f.role, model: f.model, key: f.key, tools: tools.length ? tools : ['*'] };
}

export default function AgentsSection({ setup, onChange }: { setup: SetupBody; onChange: (s: SetupBody) => void }) {
  const [f, setF] = useState(EMPTY);
  if (setup.team.kind !== 'builtin') return <SectionHead title="Agents" about="This team brings its own agents; there is nothing to set here." />;
  const team = setup.team;
  const field = (k: keyof typeof EMPTY) => ({ value: f[k], onChange: (v: string) => setF({ ...f, [k]: v }) });
  const add = () => { onChange({ ...setup, team: { ...team, agents: addTo(team.agents, agentOf(f)) } }); setF(EMPTY); };
  return (
    <div className="grid gap-4">
      <SectionHead title="Agents" about="Each agent's role and model. Keys are names from the key store; the key itself never goes in a file." />
      <Card>
        <ItemRows items={team.agents} empty="No agents yet." onRemove={(i) => onChange({ ...setup, team: { ...team, agents: removeAt(team.agents, i) } })}
          render={(a) => <span><span className="font-medium">{a.id}</span> <span className="text-faint">· {a.model} · key {a.key}</span></span>} />
      </Card>
      <Card title="Add an agent">
        <div className="grid gap-3 p-[18px] md:grid-cols-2">
          <TextField label="Agent id" mono {...field('id')} />
          <TextField label="Model" mono placeholder="claude-sonnet" {...field('model')} />
          <TextField label="Role prompt" multiline {...field('role')} />
          <TextField label="Model key name" mono placeholder="A name from the key store, never the key" {...field('key')} />
          <TextField label="Tools" mono placeholder="* for every tool, or names separated by commas" {...field('tools')} />
        </div>
        <div className="flex justify-end border-t border-line px-[18px] py-2.5"><Button disabled={!f.id || !f.model || !f.key} onClick={add}>Add agent</Button></div>
      </Card>
    </div>
  );
}
