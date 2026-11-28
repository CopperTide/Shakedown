// The world's configuration page: one section at a time, saved to its file.
import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { LoadingState } from '@/components/ui/state';
import { useProjectItems } from '@/hooks/useProjectItems';
import type { ItemSummary, WorldBody } from '@/model/project';
import DeleteItemDialog from '../DeleteItemDialog';
import EditorFrame from '../EditorFrame';
import { groupNames, scenariosIn } from '../tree';
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

interface Around { groups: string[]; scenarios: ItemSummary[] }

function Section({ id, worldId, world, onChange, onOpen, around }: { id: string; worldId: string; world: WorldBody; onChange: (w: WorldBody) => void; onOpen: (s: string) => void; around: Around }) {
  switch (id) {
    case 'connectors': return <ConnectorsSection world={world} onChange={onChange} />;
    case 'flows': return <FlowsSection world={world} onChange={onChange} />;
    case 'data': return <DataSection id={worldId} world={world} onChange={onChange} />;
    case 'people': return <PeopleSection world={world} onChange={onChange} />;
    case 'observability': return <ObservabilitySection world={world} onChange={onChange} />;
    case 'runtime': return <RuntimeSection world={world} onChange={onChange} />;
    default: return <WorldOverview world={world} onOpen={onOpen} onChange={onChange} {...around} />;
  }
}

export default function WorldEditor() {
  const { id = '' } = useParams();
  const ed = useItemEditor<WorldBody>('worlds', id);
  const [active, setActive] = useState('overview');
  const navigate = useNavigate();
  const worlds = useProjectItems('worlds').data ?? [];
  const scenarios = useProjectItems('scenarios').data ?? [];
  const around = { groups: groupNames(worlds), scenarios: scenariosIn(scenarios, id) };
  if (!ed.draft) return ed.error ? <p role="alert" className="p-7 text-alarm">{ed.error.message}</p> : <LoadingState label="Loading the world" />;
  const sections = SECTIONS.map((s) => (s.id === 'runtime' && !ed.draft?.package ? { ...s, status: 'warn' as const } : s));
  return (
    <EditorFrame title={ed.draft.label} hint="The production systems before any agent." sections={sections}
      actions={<DeleteItemDialog kind="worlds" id={id} label={ed.draft.label} onDeleted={() => navigate('/worlds')} />} active={active} onSelect={setActive}
      dirty={ed.dirty} onSave={() => { void ed.save(); }} onDiscard={ed.discard} saving={ed.saving} error={ed.error}>
      <Section id={active} worldId={id} world={ed.draft} onChange={ed.setDraft} onOpen={setActive} around={around} />
    </EditorFrame>
  );
}
