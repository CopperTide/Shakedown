// Step 2: a model to run agents and the assistant on, added right here, and what we promise about
// keys (on hovering "Your keys stay yours"). Saving a key that works finishes setup.
import { ShieldCheck } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Hint } from '@/components/ui/hint';
import { AddKeyForm, emptyKeyForm, useModelKeys, withKind } from '@/features/settings/ModelsPage';

export const PROMISES = [
  'Keys are sealed with the vault key and stored in Shakedown\'s database (hosted: CopperTide\'s database).',
  'They are never written to project files, logs, traces or recordings.',
  'Nobody can view a key again; only its last four characters show.',
  'Model calls go through Shakedown\'s gateway, so built-in, Claude Code and Codex agents never get a stored key. A key you name in an HTTP setup\'s auth is sent to that endpoint on each request.',
];

const ADD_HINT = 'Every run calls a real model. Add a provider key here, or set one as an environment variable before starting Shakedown.';

export default function StepModel({ ready }: { ready: boolean }) {
  const { form, setForm, add } = useModelKeys(withKind(emptyKeyForm(), 'openrouter'));
  return (
    <div className="grid max-w-xl gap-4 text-sm">
      {ready
        ? <p className="font-medium text-ok">✓ Model connected</p>
        : <Card title="Add a key" hint={ADD_HINT}><AddKeyForm form={form} error={add.error ? String(add.error) : null} onChange={setForm} onAdd={() => { void add.run(); }} /></Card>}
      <p className="inline-flex items-center gap-1.5 font-medium text-ok">
        <ShieldCheck aria-hidden="true" className="size-4" />
        <Hint hint={<ul className="grid gap-1">{PROMISES.map((p) => <li key={p}>{p}</li>)}</ul>}>Your keys stay yours</Hint>
      </p>
    </div>
  );
}
