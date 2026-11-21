import { X } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { Segmented } from '@/components/ui/segmented';
import { Table, Td, Th, Tr } from '@/components/ui/table';
import type { Person, RoleForm, TemplateForm } from '@/model/world';
import { type Draft, keptRoles, roleCount } from './builder';

type Edit = { form: TemplateForm; d: Draft; onDraft: (d: Draft) => void };

/** Each role kept: its count (following the size until typed), the systems it acts on and what it does. */
function Roles({ form, d, onDraft }: Edit) {
  const label = (id: string) => form.systems.find((s) => s.id === id)?.label ?? id;
  const setCount = (role: string, v: string) => onDraft({ ...d, counts: { ...d.counts, [role]: v.trim() === '' ? 0 : Math.trunc(Number(v)) } });
  return (
    <Table>
      <thead><tr><Th>Role</Th><Th className="text-right">Count</Th><Th>Acts on</Th><Th>Behaviour</Th></tr></thead>
      <tbody>
        {keptRoles(form, d).map((r) => (
          <Tr key={r.id}>
            <Td>{r.label}</Td>
            <Td className="text-right"><Input type="number" className="ml-auto w-16 text-right tabular-nums" min={r.min} aria-label={r.label}
              value={roleCount(form, d, r.id)} onChange={(e) => setCount(r.id, e.target.value)} /></Td>
            <Td className="text-muted">{r.systems.filter((s) => d.systems.includes(s)).map(label).join(', ')}</Td>
            <Td className="text-muted">{r.summary ?? ''}</Td>
          </Tr>
        ))}
      </tbody>
    </Table>
  );
}

/** A name as an input with ✕ (`title`: the person's login, when drawn). */
function Name({ name, login, onName, onRemove }: { name: string; login?: string; onName: (v: string) => void; onRemove: () => void }) {
  return (
    <div className="flex items-center gap-1">
      <Input value={name} aria-label="Name" title={login} onChange={(e) => onName(e.target.value)} />
      <Button variant="ghost" size="icon" aria-label={`Remove ${name}`} title="Remove" onClick={onRemove}><X /></Button>
    </div>
  );
}

/**
 * The names of one role: the people drawn (renamed or removed by id; `people` may be the last preview's while the next
 * is asked for, so the inputs being typed in stay), then the people added, straight from the draft; "+ Add person".
 */
function Names({ role, people, d, onDraft }: { role: RoleForm; people: Person[]; d: Draft; onDraft: (d: Draft) => void }) {
  const drawn = people.filter((p) => p.role === role.id && !p.added && !d.removed.includes(p.id));
  const added = d.added.map((a, i) => ({ ...a, i })).filter((a) => a.role === role.id);
  const logins = people.filter((p) => p.role === role.id && p.added).map((p) => p.login);
  const add = () => onDraft({ ...d, added: [...d.added, { name: `New person ${added.length + 1}`, role: role.id }] });
  const setAdded = (i: number, name: string) => onDraft({ ...d, added: d.added.map((a, j) => (j === i ? { ...a, name } : a)) });
  return (
    <div className="grid gap-2 p-3 sm:grid-cols-2 lg:grid-cols-3">
      {drawn.map((p) => (
        <Name key={p.id} name={d.edits[p.id]?.name ?? p.name} login={p.login} onName={(name) => onDraft({ ...d, edits: { ...d.edits, [p.id]: { name } } })}
          onRemove={() => onDraft({ ...d, removed: [...d.removed, p.id] })} />
      ))}
      {added.map((a, k) => (
        <Name key={`added-${a.i}`} name={a.name} login={logins[k]} onName={(name) => setAdded(a.i, name)} onRemove={() => onDraft({ ...d, added: d.added.filter((_, j) => j !== a.i) })} />
      ))}
      <button type="button" onClick={add} className="h-7 rounded-md border border-dashed border-line-strong text-sm text-muted hover:border-accent hover:text-fg">+ Add person</button>
    </div>
  );
}

/** The simulated people: the roles and their counts, then one role's names to edit (or why they can't be drawn). */
export default function PeopleStep({ form, d, onDraft, people, error = null }: Edit & { people: Person[] | null; error?: string | null }) {
  const roles = keptRoles(form, d);
  const [shown, setShown] = useState<string | null>(null);
  const role = roles.find((r) => r.id === shown) ?? roles[0];
  const total = roles.reduce((n, r) => n + roleCount(form, d, r.id), 0);
  const waiting = error ? <div className="p-3 text-sm text-alarm">{`Can't draw the names: ${error}`}</div> : <div className="p-3 text-sm text-muted">Drawing the names…</div>;
  return (
    <>
      <Panel><PanelHeader title="Simulated people" meta={total} /><Roles form={form} d={d} onDraft={onDraft} /></Panel>
      {role && (
        <Panel>
          <PanelHeader title={role.label} meta={people?.filter((p) => p.role === role.id).length}>
            {roles.length > 1 && <Segmented label="Role shown" value={role.id} options={roles.map((r) => ({ id: r.id, label: r.label }))} onChange={setShown} />}
          </PanelHeader>
          {people ? <Names role={role} people={people} d={d} onDraft={onDraft} /> : waiting}
        </Panel>
      )}
    </>
  );
}
