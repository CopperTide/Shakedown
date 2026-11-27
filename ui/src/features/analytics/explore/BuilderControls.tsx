// The builder's controls: what to measure, what to split it by (up to two), which runs count, and
// one-click starting points.
import type { DimensionId, ExploreSchema, MeasureId } from '@/api/explore';
import { NativeSelect } from '@/components/ui/input';
import { Segmented } from '@/components/ui/segmented';
import { RANGES } from '../filters';
import MultiPicker from './MultiPicker';
import { type BuilderState, MAX_MEASURES, PRESETS, withPreset } from './spec';

type OnChange = (s: BuilderState) => void;

/** "By" and "then by": the first and second dimension, each optional. */
function Dimensions({ s, schema, onChange }: { s: BuilderState; schema: ExploreSchema; onChange: OnChange }) {
  const pick = (i: number, v: string) => {
    const dims = [...s.dimensions];
    if (v) dims[i] = v as DimensionId;
    else dims.splice(i);
    onChange({ ...s, dimensions: dims.filter((d, j) => dims.indexOf(d) === j).slice(0, 2) });
  };
  const select = (i: number, label: string) => (
    <label className="flex items-center gap-1.5 text-xs text-muted">
      {label}
      <NativeSelect aria-label={label} value={s.dimensions[i] ?? ''} onChange={(e) => pick(i, e.target.value)} className="h-8 w-36 text-xs">
        <option value="">{i === 0 ? 'Nothing (totals)' : 'Nothing'}</option>
        {schema.dimensions.filter((d) => i === 0 || d.id !== s.dimensions[0]).map((d) => <option key={d.id} value={d.id} title={d.description}>{d.label}</option>)}
      </NativeSelect>
    </label>
  );
  return <>{select(0, 'By')}{s.dimensions.length > 0 && select(1, 'then by')}</>;
}

function Filters({ s, schema, onChange }: { s: BuilderState; schema: ExploreSchema; onChange: OnChange }) {
  const o = schema.options;
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Segmented label="Time range" value={s.range} options={RANGES} onChange={(range) => onChange({ ...s, range })} />
      <MultiPicker label="Experiment" options={o.experiments.map((e) => ({ id: e.id, label: e.name }))} picked={s.experiment} onChange={(experiment) => onChange({ ...s, experiment })} />
      <MultiPicker label="Setup" options={o.setups.map((v) => ({ id: v, label: v }))} picked={s.setup} onChange={(setup) => onChange({ ...s, setup })} />
      <MultiPicker label="Agent" options={o.agents.map((v) => ({ id: v, label: v }))} picked={s.agent} onChange={(agent) => onChange({ ...s, agent })} />
    </div>
  );
}

export default function BuilderControls({ s, schema, onChange }: { s: BuilderState; schema: ExploreSchema; onChange: OnChange }) {
  const label = (id: MeasureId) => schema.measures.find((m) => m.id === id)?.label ?? id;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-muted">Show</span>
        <MultiPicker label="Measures" max={MAX_MEASURES} summary={s.measures.map(label).join(', ')}
          options={schema.measures.map((m) => ({ id: m.id, label: m.label, hint: m.description }))}
          picked={s.measures} onChange={(v) => v.length && onChange({ ...s, measures: v as MeasureId[] })} />
        <Dimensions s={s} schema={schema} onChange={onChange} />
      </div>
      <Filters s={s} schema={schema} onChange={onChange} />
      <div role="group" aria-label="Starting points" className="flex flex-wrap items-center gap-1.5">
        <span className="text-2xs text-faint">Try</span>
        {PRESETS.map((p) => (
          <button key={p.label} type="button" onClick={() => onChange(withPreset(s, p))}
            className="h-6 rounded-md border border-line bg-surface px-2 text-2xs text-muted hover:border-line-strong hover:text-fg">{p.label}</button>
        ))}
      </div>
    </div>
  );
}
