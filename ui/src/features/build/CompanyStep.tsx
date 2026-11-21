import { Field, Input } from '@/components/ui/input';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { Segmented } from '@/components/ui/segmented';
import type { TemplateForm } from '@/model/world';
import type { Draft } from './builder';

const num = (v: string) => (v.trim() === '' ? NaN : Number(v));

/** The company's name and size (a preset or an exact number); the seed under Options. */
export default function CompanyStep({ form, d, onDraft }: { form: TemplateForm; d: Draft; onDraft: (d: Draft) => void }) {
  const { size } = form;
  const presets = size.presets ?? [];
  return (
    <Panel>
      <PanelHeader title="Company" />
      <div className="flex max-w-2xl flex-col gap-5 p-4">
        <Field label="Name"><Input className="h-9 text-base" value={d.company} aria-label="Company name" onChange={(e) => onDraft({ ...d, company: e.target.value })} /></Field>
        <Field label={size.label}>
          <div className="flex flex-wrap items-center gap-3">
            {presets.length > 0 && <Segmented label={size.label} value={d.size} options={presets.map((n) => ({ id: n, label: String(n) }))} onChange={(n) => onDraft({ ...d, size: n, counts: {} })} />}
            <Input type="number" className="w-20 tabular-nums" value={Number.isNaN(d.size) ? '' : d.size} min={size.min} max={size.max} title={`Between ${size.min} and ${size.max}`}
              aria-label={`${size.label}, exact`} onChange={(e) => onDraft({ ...d, size: num(e.target.value), counts: {} })} />
          </div>
        </Field>
        <details className="text-xs text-muted">
          <summary className="cursor-pointer">Options</summary>
          <Field label="Seed" className="mt-2 w-32"><Input type="number" className="font-mono" value={d.seed} onChange={(e) => onDraft({ ...d, seed: Math.trunc(num(e.target.value)) || 0 })} /></Field>
        </details>
      </div>
    </Panel>
  );
}
