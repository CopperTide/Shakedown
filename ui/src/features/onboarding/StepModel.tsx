// Step 2: a model to run agents and the assistant on, and what we promise about keys.
import { Link } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

const PROMISES = [
  'Keys are sealed with the vault key and stored in Shakedown\'s database (hosted: CopperTide\'s database).',
  'They are never written to project files, logs, traces or recordings.',
  'Nobody can view a key again; only its last four characters show.',
  'Model calls go through Shakedown\'s gateway, so built-in, Claude Code and Codex agents never get a stored key. A key you name in an HTTP setup\'s auth is sent to that endpoint on each request.',
];

export default function StepModel({ ready }: { ready: boolean }) {
  return (
    <div className="grid max-w-xl gap-4 text-sm">
      <p className="text-muted">{ready ? 'A model provider is connected. Every run calls a real model on it.' : 'Every run calls a real model. Set a provider key as an environment variable before starting Shakedown, or add one in Connections & models.'}</p>
      <Card title="Your keys stay yours" className="border-ok">
        <ul className="grid gap-1 px-[18px] py-3">{PROMISES.map((p) => <li key={p}>{p}</li>)}</ul>
      </Card>
      <Button asChild className="justify-self-start"><Link to="/settings/models">Open Connections &amp; models</Link></Button>
    </div>
  );
}
