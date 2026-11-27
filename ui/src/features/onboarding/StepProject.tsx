// Step 1: the project. Created on a click, or explained when shaked has no folder. Git is offered
// only where the project is a folder on disk (hosted projects live in a database).
import { useState } from 'react';
import { createProject } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Toggle } from '@/components/ui/toggle';
import { errorMessage } from '@/lib/errors';
import type { ProjectStage } from './steps';

export default function StepProject({ stage, name, folder, onCreated }: { stage: ProjectStage; name?: string; folder: boolean; onCreated: () => void }) {
  const [label, setLabel] = useState('');
  const [git, setGit] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (stage === 'open') return <p className="text-sm">{name} is ready.</p>;
  if (stage === 'not_configured') return (
    <p className="max-w-prose text-sm text-muted">Shakedown was started without a project folder. Start it again with <code className="font-mono">SHAKEDOWN_PROJECT</code> set to the folder your project should live in; it is created if it doesn't exist.</p>
  );
  const create = async () => { try { await createProject(label.trim(), folder && git); onCreated(); } catch (e) { setError(errorMessage(e)); } };
  return (
    <div className="grid max-w-md gap-4">
      <label className="grid gap-1 text-sm"><span className="text-muted">Project name</span><Input aria-label="Project name" value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Larkspur Ops" /></label>
      {folder && <Toggle label="Track with git" description="Commit worlds, scenarios and setups like code." checked={git} onChange={setGit} />}
      {error && <p role="alert" className="text-sm text-alarm">{error}</p>}
      <Button variant="primary" disabled={!label.trim()} onClick={() => { void create(); }}>Create project</Button>
    </div>
  );
}
