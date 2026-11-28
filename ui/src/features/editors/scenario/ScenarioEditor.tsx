// The scenario's configuration page: inputs, events, gates and checks, saved to its file, and the
// experiments that play it.
import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { LoadingState } from '@/components/ui/state';
import { useProjectItems } from '@/hooks/useProjectItems';
import type { ItemSummary, ScenarioBody } from '@/model/project';
import DeleteItemDialog from '../DeleteItemDialog';
import EditorFrame from '../EditorFrame';
import { experimentsUsing } from '../tree';
import { useItemEditor } from '../useItemEditor';
import ChecksSection from './ChecksSection';
import EventsSection from './EventsSection';
import GatesSection from './GatesSection';
import InputsSection from './InputsSection';
import ScenarioExperiments from './ScenarioExperiments';
import ScenarioOverview from './ScenarioOverview';

const SECTIONS = [{ id: 'overview', label: 'Overview' }, { id: 'inputs', label: 'Inputs' }, { id: 'events', label: 'Events' }, { id: 'gates', label: 'Gates' }, { id: 'checks', label: 'Checks' }];

/** The sections, the last one the experiments playing the scenario with their count. */
const sectionsWith = (experiments: number) => [...SECTIONS, { id: 'experiments', label: `Experiments (${experiments})` }];

function Section({ id, scenario, onChange, experiments }: { id: string; scenario: ScenarioBody; onChange: (s: ScenarioBody) => void; experiments: ItemSummary[] }) {
  switch (id) {
    case 'inputs': return <InputsSection scenario={scenario} onChange={onChange} />;
    case 'events': return <EventsSection scenario={scenario} onChange={onChange} />;
    case 'gates': return <GatesSection scenario={scenario} onChange={onChange} />;
    case 'checks': return <ChecksSection scenario={scenario} onChange={onChange} />;
    case 'experiments': return <ScenarioExperiments experiments={experiments} />;
    default: return <ScenarioOverview scenario={scenario} experiments={experiments} />;
  }
}

export default function ScenarioEditor() {
  const { id = '' } = useParams();
  const ed = useItemEditor<ScenarioBody>('scenarios', id);
  const [active, setActive] = useState('overview');
  const navigate = useNavigate();
  const experiments = experimentsUsing(useProjectItems('experiments').data ?? [], id);
  if (!ed.draft) return ed.error ? <p role="alert" className="p-7 text-alarm">{ed.error.message}</p> : <LoadingState label="Loading the scenario" />;
  return (
    <EditorFrame title={ed.draft.label} hint="An incident or test inside one world." sections={sectionsWith(experiments.length)}
      actions={<DeleteItemDialog kind="scenarios" id={id} label={ed.draft.label} onDeleted={() => navigate('/scenarios')} />} active={active} onSelect={setActive}
      dirty={ed.dirty} onSave={() => { void ed.save(); }} onDiscard={ed.discard} saving={ed.saving} error={ed.error}>
      <Section id={active} scenario={ed.draft} onChange={ed.setDraft} experiments={experiments} />
    </EditorFrame>
  );
}
