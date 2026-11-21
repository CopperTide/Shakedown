// The scenario's configuration page: inputs, events, gates and checks, saved to its file.
import { useState } from 'react';
import { useParams } from 'react-router-dom';
import { LoadingState } from '@/components/ui/state';
import type { ScenarioBody } from '@/model/project';
import EditorFrame from '../EditorFrame';
import { useItemEditor } from '../useItemEditor';
import ChecksSection from './ChecksSection';
import EventsSection from './EventsSection';
import GatesSection from './GatesSection';
import InputsSection from './InputsSection';
import ScenarioOverview from './ScenarioOverview';

const SECTIONS = [{ id: 'overview', label: 'Overview' }, { id: 'inputs', label: 'Inputs' }, { id: 'events', label: 'Events' }, { id: 'gates', label: 'Gates' }, { id: 'checks', label: 'Checks' }];

function Section({ id, scenario, onChange }: { id: string; scenario: ScenarioBody; onChange: (s: ScenarioBody) => void }) {
  switch (id) {
    case 'inputs': return <InputsSection scenario={scenario} onChange={onChange} />;
    case 'events': return <EventsSection scenario={scenario} onChange={onChange} />;
    case 'gates': return <GatesSection scenario={scenario} onChange={onChange} />;
    case 'checks': return <ChecksSection scenario={scenario} onChange={onChange} />;
    default: return <ScenarioOverview scenario={scenario} />;
  }
}

export default function ScenarioEditor() {
  const { id = '' } = useParams();
  const ed = useItemEditor<ScenarioBody>('scenarios', id);
  const [active, setActive] = useState('overview');
  if (!ed.draft) return ed.error ? <p role="alert" className="p-7 text-alarm">{ed.error.message}</p> : <LoadingState label="Loading the scenario" />;
  return (
    <EditorFrame title={ed.draft.label} meta="An incident or test inside one world." sections={SECTIONS} active={active} onSelect={setActive}
      dirty={ed.dirty} onSave={() => { void ed.save(); }} onDiscard={ed.discard} saving={ed.saving} error={ed.error}>
      <Section id={active} scenario={ed.draft} onChange={ed.setDraft} />
    </EditorFrame>
  );
}
