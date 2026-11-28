import { Hint } from '@/components/ui/hint';
import { Checkbox, Field, Input, NativeSelect } from '@/components/ui/input';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { Tag } from '@/components/ui/status';
import type { EventForm, TemplateForm } from '@/model/world';
import type { Draft } from './builder';

type Choice = Draft['events'][string];

/** An event as the draft has it: on or off (the template's default until changed) and its params. */
export const eventChoice = (d: Draft, e: EventForm): Choice => d.events[e.id] ?? { on: e.default, params: {} };

/** The events whose systems are all chosen. */
export const offeredEvents = (form: TemplateForm, d: Draft) => form.events.filter((e) => e.needs.every((s) => d.systems.includes(s)));

/** The event's description with its params filled in. */
export function described(e: EventForm, c: Choice): string {
  const value = (name: string) => c.params[name] ?? e.params.find((p) => p.name === name)?.default ?? name;
  return e.description.replace(/\{param\.([\w-]+)\}/g, (_, name: string) => String(value(name)));
}

/** One event's params, inline: a number or one of its choices. */
function Params({ e, c, set }: { e: EventForm; c: Choice; set: (c: Choice) => void }) {
  const put = (name: string, v: number | string) => set({ ...c, params: { ...c.params, [name]: v } });
  return e.params.map((p) => {
    const v = c.params[p.name] ?? p.default;
    return (
      <Field key={p.name} label={p.label} className="w-28">
        {p.kind === 'choice'
          ? <NativeSelect value={String(v)} disabled={!c.on} onChange={(ev) => put(p.name, ev.target.value)}>{(p.choices ?? []).map((x) => <option key={String(x)} value={String(x)}>{String(x)}</option>)}</NativeSelect>
          : <Input type="number" className="tabular-nums" value={v} min={p.min} max={p.max} disabled={!c.on} onChange={(ev) => put(p.name, Number(ev.target.value))} />}
      </Field>
    );
  });
}

/** The events baked into the world: each ticked on or off, a trap or an event, with its params. */
export default function EventsStep({ form, d, onDraft }: { form: TemplateForm; d: Draft; onDraft: (d: Draft) => void }) {
  const offered = offeredEvents(form, d);
  const set = (id: string, c: Choice) => onDraft({ ...d, events: { ...d.events, [id]: c } });
  return (
    <Panel>
      <PanelHeader title="Events" meta={`${offered.filter((e) => eventChoice(d, e).on).length} of ${offered.length}`} />
      <ul className="divide-y divide-line">
        {offered.map((e) => {
          const c = eventChoice(d, e);
          return (
            <li key={e.id} className="flex flex-wrap items-start gap-3 px-4 py-3">
              <Checkbox className="mt-1" checked={c.on} aria-label={e.label} onChange={(ev) => set(e.id, { ...c, on: ev.target.checked })} />
              <div className="min-w-0 flex-1">
                <div className={c.on ? 'text-sm font-medium' : 'text-sm font-medium text-muted'}><Hint hint={described(e, c)}>{e.label}</Hint></div>
              </div>
              <div className="flex flex-wrap items-end gap-2"><Tag>{e.kind === 'trap' ? 'Trap' : 'Event'}</Tag><Params e={e} c={c} set={(x) => set(e.id, x)} /></div>
            </li>
          );
        })}
      </ul>
    </Panel>
  );
}
