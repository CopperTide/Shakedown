// Steps 3–5: a world, a setup and a scenario, each made in its editor or drafted by the assistant.
import { Sparkles } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { KINDS } from '@/features/editors/kinds';
import NewItemDialog from '@/features/editors/NewItemDialog';
import { ASK_EVENT } from '@/features/shell/TopBar';
import type { ItemKind } from '@/model/project';

export default function StepItems({ kind, about, prompt, count }: { kind: ItemKind; about: string; prompt: string; count: number }) {
  const navigate = useNavigate();
  const ask = () => window.dispatchEvent(new CustomEvent(ASK_EVENT, { detail: { prompt } }));
  return (
    <div className="grid max-w-xl gap-4 text-sm">
      <p className="text-muted">{about}</p>
      {count > 0 && <p>You have {count} {count === 1 ? KINDS[kind].one : KINDS[kind].many.toLowerCase()}. Add more any time.</p>}
      <div className="flex flex-wrap gap-2">
        <NewItemDialog kind={kind} taken={[]} onCreated={(id) => navigate(`${KINDS[kind].route}/${id}`)} />
        <Button onClick={ask}><Sparkles className="text-copper" />Describe it to the assistant</Button>
      </div>
    </div>
  );
}
