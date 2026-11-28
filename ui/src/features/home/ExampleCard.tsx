// "Start from an example": on Home while the project has no worlds. One click adds a small, real
// example (project.example) and opens its experiment, so the next click is Deploy. Nothing is added
// until the button is pressed.
import { Sparkles } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { toast } from 'sonner';
import { addExample } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { useAction } from '@/hooks/useAction';
import { projectChanged } from '@/hooks/useProjectItems';

/** Adds the example, says what a run of it shows, and opens its experiment. */
function useAddExample() {
  const navigate = useNavigate();
  return useAction(async () => {
    const added = await addExample(true);
    projectChanged();
    toast('Example added: press Deploy to run it', { description: added.shows, duration: 12_000 });
    navigate(`/experiments/file/${encodeURIComponent(added.experiment)}`);
  }, { stayBusy: true });
}

export default function ExampleCard() {
  const add = useAddExample();
  return (
    <Card title="Start from an example" description="See Shakedown work before you build your own world.">
      <div className="grid gap-3 px-[18px] py-4 text-sm">
        <p>
          Adds a tiny retail order desk to your project: a world, one customer ticket and one agent on the model you
          connected, with an experiment ready to deploy. A run takes about a minute, and you can edit or delete it all
          afterwards.
        </p>
        {add.error && <p role="alert" className="text-alarm">{add.error}</p>}
        <Button variant="primary" className="justify-self-start" onClick={() => { void add.run(); }} disabled={add.busy}>
          <Sparkles aria-hidden="true" />{add.busy ? 'Adding the example…' : 'Add the example'}
        </Button>
      </div>
    </Card>
  );
}
