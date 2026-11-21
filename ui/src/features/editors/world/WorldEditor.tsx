// The world's configuration page: one section at a time, saved to its file.
import { useState } from 'react';
import { useParams } from 'react-router-dom';
import { LoadingState } from '@/components/ui/state';
import type { WorldBody } from '@/model/project';
import EditorFrame from '../EditorFrame';
import { useItemEditor } from '../useItemEditor';
import ConnectorsSection from './ConnectorsSection';
import DataSection from './DataSection';
import FlowsSection from './FlowsSection';
import ObservabilitySection from './ObservabilitySection';
import PeopleSection from './PeopleSection';
import RuntimeSection from './RuntimeSection';
import WorldOverview from './WorldOverview';

const SECTIONS = [
  { id: 'overview', label: 'Overview' }, { id: 'connectors', label: 'Connectors' }, { id: 'flows', label: 'Software flows' },
  { id: 'data', label: 'Data' }, { id: 'people', label: 'People' }, { id: 'observability', label: 'Observability & on-call' }, { id: 'runtime', label: 'Runtime' },
];

function Section({ id, world, onChange, onOpen }: { id: string; world: WorldBody; onChange: (w: WorldBody) => void; onOpen: (s: string) => void }) {
  switch (id) {
    case 'connectors': return <ConnectorsSection world={world} onChange={onChange} />;
    case 'flows': return <FlowsSection world={world} onChange={onChange} />;
    case 'data': return <DataSection world={world} onChange={onChange} />;
    case 'people': return <PeopleSection world={world} onChange={onChange} />;
    case 'observability': return <ObservabilitySection world={world} onChange={onChange} />;
    case 'runtime': return <RuntimeSection world={world} onChange={onChange} />;
    default: return <WorldOverview world={world} onOpen={onOpen} />;
  }
}

export default function WorldEditor() {
  const { id = '' } = useParams();
  const ed = useItemEditor<WorldBody>('worlds', id);
  const [active, setActive] = useState('overview');
  if (!ed.draft) return ed.error ? <p role="alert" className="p-7 text-alarm">{ed.error.message}</p> : <LoadingState label="Loading the world" />;
  const sections = SECTIONS.map((s) => (s.id === 'runtime' && !ed.draft?.package ? { ...s, status: 'warn' as const } : s));
  return (
    <EditorFrame title={ed.draft.label} meta="The production systems before any agent." sections={sections} active={active} onSelect={setActive}
      dirty={ed.dirty} onSave={() => { void ed.save(); }} onDiscard={ed.discard} saving={ed.saving} error={ed.error}>
      <Section id={active} world={ed.draft} onChange={ed.setDraft} onOpen={setActive} />
    </EditorFrame>
  );
}
