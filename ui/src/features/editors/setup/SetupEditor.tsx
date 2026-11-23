// The agent setup's configuration page: how the team runs and its agents, saved to its file.
import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { LoadingState } from '@/components/ui/state';
import type { SetupBody } from '@/model/project';
import DeleteItemDialog from '../DeleteItemDialog';
import EditorFrame from '../EditorFrame';
import { useItemEditor } from '../useItemEditor';
import AgentsSection from './AgentsSection';
import MadlSection from './MadlSection';
import TeamKindSection from './TeamKindSection';

const SECTIONS = [{ id: 'kind', label: 'How it runs' }, { id: 'agents', label: 'Agents' }];

export default function SetupEditor() {
  const { id = '' } = useParams();
  const ed = useItemEditor<SetupBody>('setups', id);
  const [active, setActive] = useState('kind');
  const navigate = useNavigate();
  if (!ed.draft) return ed.error ? <p role="alert" className="p-7 text-alarm">{ed.error.message}</p> : <LoadingState label="Loading the setup" />;
  const sections = ed.draft.team.kind === 'madl' ? [...SECTIONS, { id: 'graph', label: 'Team graph' }] : SECTIONS;
  return (
    <EditorFrame title={ed.draft.label} meta="An agent team to test, built any way." sections={sections}
      actions={<DeleteItemDialog kind="setups" id={id} label={ed.draft.label} onDeleted={() => navigate('/setups')} />} active={active} onSelect={setActive}
      dirty={ed.dirty} onSave={() => { void ed.save(); }} onDiscard={ed.discard} saving={ed.saving} error={ed.error}>
      {active === 'graph' ? <MadlSection id={id} /> : active === 'agents' ? <AgentsSection setup={ed.draft} onChange={ed.setDraft} /> : <TeamKindSection setup={ed.draft} onChange={ed.setDraft} />}
    </EditorFrame>
  );
}
