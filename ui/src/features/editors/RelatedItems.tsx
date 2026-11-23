// What belongs to the item on screen, each a link to its page: a world's scenarios, a scenario's experiments.
import { Link } from 'react-router-dom';
import { Card } from '@/components/ui/card';
import type { ItemSummary } from '@/model/project';

/** `noun` names one item, e.g. `scenario`, for the count. */
export default function RelatedItems({ title, noun, items, to, empty }: { title: string; noun: string; items: ItemSummary[]; to: (id: string) => string; empty: string }) {
  return (
    <Card title={title} description={items.length ? `${items.length} ${noun}${items.length === 1 ? '' : 's'}` : undefined}>
      {items.length === 0
        ? <p className="px-[18px] py-3 text-sm text-faint">{empty}</p>
        : (
          <ul className="divide-y divide-line">
            {items.map((i) => <li key={i.id}><Link to={to(i.id)} className="block px-[18px] py-2.5 text-sm font-medium transition-colors hover:bg-hover">{i.label}</Link></li>)}
          </ul>
        )}
    </Card>
  );
}
