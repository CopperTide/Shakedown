// Project settings: its name, the assistant's model, what agents may do without asking, how to
// use Shakedown from an agent, and undoing the last delete.
import { useEffect, useState } from 'react';
import { updateSettings } from '@/api/project';
import { Card } from '@/components/ui/card';
import { FormRow } from '@/components/ui/form-row';
import { Input } from '@/components/ui/input';
import { Segmented } from '@/components/ui/segmented';
import { LoadingState } from '@/components/ui/state';
import EditorFrame from '@/features/editors/EditorFrame';
import { SectionHead } from '@/features/editors/fields';
import { useProject } from '@/hooks/useProject';
import { errorMessage } from '@/lib/errors';
import { type SettingsForm, formFrom, settingsFrom } from './project';
import LastDelete from './LastDelete';
import UseFromAgent from './UseFromAgent';

const SECTIONS = [{ id: 'project', label: 'Project' }, { id: 'assistant', label: 'Assistant' }, { id: 'permissions', label: 'What agents may do' }, { id: 'mcp', label: 'Use from an agent' }, { id: 'trash', label: 'Deleted files' }];

function Permissions({ f, set }: { f: SettingsForm; set: (p: Partial<SettingsForm>) => void }) {
  return (
    <Card>
      <div className="px-[18px]">
        <FormRow label="Edit project files" help="Worlds, scenarios, setups, experiments. Every edit is a normal file change you can undo.">
          <Segmented label="Edits" value={f.edit} onChange={(v) => set({ edit: v })} options={[{ id: 'diff_first', label: 'Show a diff first' }, { id: 'apply_then_tell', label: 'Apply, then tell me' }]} />
        </FormRow>
        <FormRow label="Start runs or spend money" help="Deploys, forks, dry runs, acceptance tests.">
          <Segmented label="Spending" value={f.spendMode} onChange={(v) => set({ spendMode: v })} options={[{ id: 'always_ask', label: 'Always ask' }, { id: 'under_cap', label: 'Allow under a daily cap' }]} />
          {f.spendMode === 'under_cap' && <Input aria-label="Daily cap in US dollars" value={f.cap} onChange={(e) => set({ cap: e.target.value })} className="max-w-[200px]" placeholder="USD a day" />}
        </FormRow>
        <FormRow label="Delete" help="Worlds, scenarios, setups, experiments."><span className="text-sm text-muted">Always asks</span></FormRow>
      </div>
    </Card>
  );
}

function Section({ id, f, set }: { id: string; f: SettingsForm; set: (p: Partial<SettingsForm>) => void }) {
  if (id === 'permissions') return <><SectionHead title="What agents may do without asking" about="Applies to the assistant and to every MCP client." /><Permissions f={f} set={set} /></>;
  if (id === 'mcp') return <UseFromAgent />;
  if (id === 'trash') return <LastDelete />;
  if (id === 'assistant') return (
    <><SectionHead title="Assistant" about="The chat on every page, on your own model key." />
      <Card><div className="px-[18px]">
        <FormRow label="Model" help="Empty: the default provider's default model."><Input aria-label="Model" value={f.model} onChange={(e) => set({ model: e.target.value })} placeholder="claude-sonnet" /></FormRow>
        <FormRow label="Provider" help="anthropic, openai, openrouter or local:<name>."><Input aria-label="Provider" value={f.provider} onChange={(e) => set({ provider: e.target.value })} /></FormRow>
      </div></Card></>);
  return <><SectionHead title="Project" about="The project is a folder of files. Personal use; nothing is shared unless you push it." />
    <Card><div className="px-[18px]"><FormRow label="Name" help="Shown in the sidebar and on reports."><Input aria-label="Project name" value={f.name} onChange={(e) => set({ name: e.target.value })} /></FormRow></div></Card></>;
}

export default function ProjectSettingsPage() {
  const project = useProject();
  const settings = project.data?.open ? project.data.status.settings : undefined;
  const [f, setF] = useState<SettingsForm | null>(null);
  const [active, setActive] = useState('project');
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { if (settings) setF(formFrom(settings)); }, [settings]);
  if (!settings || !f) return <LoadingState label="Loading the project's settings" />;
  const save = async () => { try { await updateSettings(settingsFrom(settings, f)); setError(null); project.reload(); } catch (e) { setError(errorMessage(e)); } };
  return (
    <EditorFrame title="Project settings" sections={SECTIONS} active={active} onSelect={setActive} dirty={JSON.stringify(f) !== JSON.stringify(formFrom(settings))}
      onSave={() => { void save(); }} onDiscard={() => setF(formFrom(settings))} saving={false} error={error ? { message: error } : null}>
      <Section id={active} f={f} set={(p) => setF({ ...f, ...p })} />
    </EditorFrame>
  );
}
