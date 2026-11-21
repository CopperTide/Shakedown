// The agent setup's configuration page: how the team runs and its agents, saved to its file.
import { useState } from 'react';
import { useParams } from 'react-router-dom';
import { LoadingState } from '@/components/ui/state';
import type { SetupBody } from '@/model/project';
import EditorFrame from '../EditorFrame';
import { useItemEditor } from '../useItemEditor';
import AgentsSection from './AgentsSection';
import TeamKindSection from './TeamKindSection';

const SECTIONS = [{ id: 'kind', label: 'How it runs' }, { id: 'agents', label: 'Agents' }];

export default function SetupEditor() {
  const { id = '' } = useParams();
  const ed = useItemEditor<SetupBody>('setups', id);
  const [active, setActive] = useState('kind');
  if (!ed.draft) return ed.error ? <p role="alert" className="p-7 text-alarm">{ed.error.message}</p> : <LoadingState label="Loading the setup" />;
  return (
    <EditorFrame title={ed.draft.label} meta="An agent team to test, built any way." sections={SECTIONS} active={active} onSelect={setActive}
      dirty={ed.dirty} onSave={() => { void ed.save(); }} onDiscard={ed.discard} saving={ed.saving} error={ed.error}>
      {active === 'agents' ? <AgentsSection setup={ed.draft} onChange={ed.setDraft} /> : <TeamKindSection setup={ed.draft} onChange={ed.setDraft} />}
    </EditorFrame>
  );
}
