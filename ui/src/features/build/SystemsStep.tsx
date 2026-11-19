import { Checkbox } from '@/components/ui/input';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { Segmented } from '@/components/ui/segmented';
import { Tag } from '@/components/ui/status';
import type { SystemForm, TemplateForm } from '@/model/world';
import { type Draft, memoryLine, toggleSystem } from './builder';

const word = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** A system's options, each a segmented choice. */
function Options({ s, d, onDraft }: { s: SystemForm; d: Draft; onDraft: (d: Draft) => void }) {
  const set = (name: string, v: string) => onDraft({ ...d, options: { ...d.options, [s.id]: { ...d.options[s.id], [name]: v } } });
  return s.options.map((o) => (
    <Segmented key={o.name} label={o.label} value={d.options[s.id]?.[o.name] ?? o.default} options={o.choices.map((c) => ({ id: c, label: word(c) }))}
      onChange={(v) => set(o.name, v)} disabled={!d.systems.includes(s.id)} />
  ));
}

/** Every system the template offers: ticked when chosen (a required one always), what it is, its options. */
export default function SystemsStep({ form, d, onDraft }: { form: TemplateForm; d: Draft; onDraft: (d: Draft) => void }) {
  return (
    <>
      <Panel>
        <PanelHeader title="Systems" meta={`${d.systems.length} of ${form.systems.length}`} />
        <ul className="divide-y divide-line">
          {form.systems.map((s) => (
            <li key={s.id} className="flex flex-wrap items-start gap-3 px-4 py-3">
              <Checkbox className="mt-1" checked={d.systems.includes(s.id)} disabled={s.required} aria-label={s.label}
                title={s.required ? 'Every world of this template has it' : undefined} onChange={(e) => onDraft(toggleSystem(d, form, s.id, e.target.checked))} />
              <div className="min-w-0 flex-1">
                <div className="text-sm font-medium">{s.label}</div>
                <div className="text-xs text-muted">{s.summary ?? s.product}</div>
              </div>
              <div className="flex flex-wrap items-center gap-2"><Options s={s} d={d} onDraft={onDraft} />{s.summary && <Tag>{s.product}</Tag>}</div>
            </li>
          ))}
        </ul>
      </Panel>
      <div className="text-xs text-muted">{memoryLine(form, d)}</div>
    </>
  );
}
