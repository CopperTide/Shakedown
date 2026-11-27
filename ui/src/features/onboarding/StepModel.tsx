// Step 2: a model to run agents and the assistant on, added right here, and what we promise about
// keys. Saving a key that works finishes setup.
import { Card } from '@/components/ui/card';
import { AddKeyForm, emptyKeyForm, useModelKeys, withKind } from '@/features/settings/ModelsPage';

const PROMISES = [
  'Keys are sealed with the vault key and stored in Shakedown\'s database (hosted: CopperTide\'s database).',
  'They are never written to project files, logs, traces or recordings.',
  'Nobody can view a key again; only its last four characters show.',
  'Model calls go through Shakedown\'s gateway, so built-in, Claude Code and Codex agents never get a stored key. A key you name in an HTTP setup\'s auth is sent to that endpoint on each request.',
];

export default function StepModel({ ready }: { ready: boolean }) {
  const { form, setForm, add } = useModelKeys(withKind(emptyKeyForm(), 'openrouter'));
  return (
    <div className="grid max-w-xl gap-4 text-sm">
      <p className="text-muted">{ready ? 'A model provider is connected. Every run calls a real model on it.' : 'Every run calls a real model. Add a provider key here (or set one as an environment variable before starting Shakedown).'}</p>
      {!ready && <Card title="Add a key"><AddKeyForm form={form} error={add.error ? String(add.error) : null} onChange={setForm} onAdd={() => { void add.run(); }} /></Card>}
      <Card title="Your keys stay yours" className="border-ok">
        <ul className="grid gap-1 px-[18px] py-3">{PROMISES.map((p) => <li key={p}>{p}</li>)}</ul>
      </Card>
    </div>
  );
}
