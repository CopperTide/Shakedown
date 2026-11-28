import { useMemo, useState } from 'react';
import { Hint } from '@/components/ui/hint';
import { Checkbox, Field, Input, NativeSelect } from '@/components/ui/input';
import { Tag } from '@/components/ui/status';
import { useBenchmark } from '@/hooks/useBenchmark';
import { cn } from '@/lib/cn';
import type { DisruptionOption, ParamSpec, ParamValue, ScenarioOptions } from '@/model/scenario';
import { type Selection, chosen, initialSelection, optionWords, paramProblem, previewText, scenarioOf, scenarioProblem, toggled, withParam } from './scenario';

/** The benchmark's `scenario_options`; the selection starts at their defaults until it is changed. */
export function useScenario(tasks: string[]) {
  const { data: bench, error } = useBenchmark();
  const options: ScenarioOptions | null = bench?.scenario_options ?? null;
  const initial = useMemo<Selection>(() => (bench ? initialSelection(bench.scenario_options ?? null) : {}), [bench]);
  const [edited, setEdited] = useState<Selection | null>(null);
  const sel = edited ?? initial;
  return {
    options, sel, error,
    toggle: (id: string, on: boolean) => setEdited((s) => toggled(s ?? initial, id, on)),
    setParam: (id: string, spec: ParamSpec, raw: string) => setEdited((s) => withParam(s ?? initial, id, spec, raw)),
    scenario: scenarioOf(options, sel),
    problem: scenarioProblem(options, sel, tasks),
  };
}

export type ScenarioState = ReturnType<typeof useScenario>;

export function ParamInput({ spec, value, onChange, disabled }: { spec: ParamSpec; value: ParamValue | undefined; onChange: (raw: string) => void; disabled: boolean }) {
  const bad = paramProblem(spec, value);
  return (
    <Field label={spec.label} className={spec.kind === 'int' ? 'w-36' : 'w-52'}>
      {spec.kind === 'choice'
        ? <NativeSelect value={String(value ?? '')} onChange={(e) => onChange(e.target.value)} disabled={disabled}>{spec.choices?.map((c) => <option key={String(c)} value={String(c)}>{String(c)}</option>)}</NativeSelect>
        : <Input type="number" min={spec.min} max={spec.max} value={String(value ?? '')} onChange={(e) => onChange(e.target.value)} disabled={disabled} aria-invalid={!!bad} className={cn(bad && 'border-alarm')} />}
    </Field>
  );
}

function DisruptionRow({ d, s, busy }: { d: DisruptionOption; s: ScenarioState; busy: boolean }) {
  const on = !!s.sel[d.id]?.on;
  const words = optionWords(d, s.sel[d.id]?.params ?? {});
  return (
    <div className={cn('flex flex-col gap-2 border-b border-line px-2.5 py-2 last:border-b-0', on && 'bg-raised')}>
      <label className="flex cursor-pointer items-start gap-2">
        <Checkbox className="mt-0.5" checked={on} onChange={(e) => s.toggle(d.id, e.target.checked)} disabled={busy} />
        <span className="flex min-w-0 flex-col">
          <span className="flex flex-wrap items-center gap-1.5 text-sm font-medium"><Hint hint={words.description}>{words.label}</Hint><Tag>{d.part}</Tag></span>
        </span>
      </label>
      {on && d.params.length > 0 && (
        <div className="flex flex-wrap gap-2 pl-5.5">
          {d.params.map((p) => <ParamInput key={p.name} spec={p} value={s.sel[d.id].params[p.name]} onChange={(raw) => s.setParam(d.id, p, raw)} disabled={busy} />)}
        </div>
      )}
    </div>
  );
}

/** The plain-words preview of what will happen, and the problem that blocks starting. */
function Preview({ s }: { s: ScenarioState }) {
  const any = chosen(s.options, s.sel).length > 0;
  return (
    <div className="flex flex-col gap-1 rounded-md border border-line bg-sunken px-2.5 py-2">
      <span className="text-2xs font-medium text-muted">What will happen</span>
      <p className={cn('text-sm', any ? 'font-medium' : 'text-muted')}>{any && <span className="mr-1 font-mono text-warn">▼</span>}{previewText(s.options, s.sel)}</p>
      {s.problem && <p className="text-xs font-medium text-alarm" role="status">⚠ {s.problem}</p>}
    </div>
  );
}

/** Disruptions to schedule inside the simulated parts, drawn from the benchmark's `scenario_options`; nothing when it offers none. */
export default function ScenarioBuilder({ s, busy }: { s: ScenarioState; busy: boolean }) {
  if (s.error) return <p className="text-xs text-muted"><Hint hint="The run starts without any.">{`Couldn't load the disruptions (${s.error})`}</Hint></p>;
  if (!s.options?.disruptions.length) return null;
  return (
    <fieldset className="flex flex-col gap-1.5">
      <legend className="mb-1 text-2xs font-medium text-muted"><Hint hint="Scheduled inside the simulated parts.">Disruptions</Hint></legend>
      <div className="rounded-md border border-line bg-surface">
        {s.options.disruptions.map((d) => <DisruptionRow key={d.id} d={d} s={s} busy={busy} />)}
      </div>
      <Preview s={s} />
    </fieldset>
  );
}
