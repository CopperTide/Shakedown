// Step 6: what is set up, the next thing to do, and Shakedown from an agent.
import { Check, Circle } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import type { StepState } from './steps';

export default function StepReady({ steps }: { steps: StepState[] }) {
  return (
    <div className="grid max-w-xl gap-4 text-sm">
      <ul className="grid gap-2">
        {steps.filter((s) => s.id !== 'ready').map((s) => (
          <li key={s.id} className="flex items-center gap-2">{s.done ? <Check className="size-4 text-ok" /> : <Circle className="size-4 text-faint" />}{s.title}</li>
        ))}
      </ul>
      <Button variant="primary" asChild className="justify-self-start"><Link to="/experiments/new">Create an experiment</Link></Button>
      <p className="text-muted">Use Shakedown from Claude Code: <code className="font-mono">claude mcp add shakedown -- shaked mcp</code></p>
    </div>
  );
}
