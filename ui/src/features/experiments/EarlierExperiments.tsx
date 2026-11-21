import { Link } from 'react-router-dom';
import { Card } from '@/components/ui/card';
import type { ListedExperiment } from '@/model/experiment';
import MoveIntoProject from './MoveIntoProject';

/** Experiments made before the project folder, read-only until they are moved into it. */
export default function EarlierExperiments({ list, onMoved }: { list: ListedExperiment[]; onMoved: () => void }) {
  if (!list.length) return null;
  return (
    <Card title="Earlier experiments" description="Made before the project folder. They stay readable here." actions={<MoveIntoProject onMoved={onMoved} />}>
      <ul className="divide-y divide-line">
        {list.map((e) => (
          <li key={e.id} className="flex items-center gap-3 px-[18px] py-2.5 text-sm">
            <Link to={`/experiments/${e.id}`} className="flex-1 font-medium hover:text-accent">{e.name}</Link>
            <span className="text-xs text-faint">{e.benchmark} · {e.runs} runs per setup</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}
