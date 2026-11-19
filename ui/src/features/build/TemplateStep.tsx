import { Tag } from '@/components/ui/status';
import { cn } from '@/lib/cn';
import type { TemplateSummary } from '@/model/world';

/** The templates as cards: name, summary and its systems; the chosen one ringed. */
export default function TemplateStep({ templates, chosen, onChoose }: { templates: TemplateSummary[]; chosen: string | null; onChoose: (id: string) => void }) {
  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {templates.map((t) => (
        <button key={t.id} type="button" aria-pressed={t.id === chosen} onClick={() => onChoose(t.id)}
          className={cn('flex flex-col gap-2 rounded-lg border bg-surface p-4 text-left shadow-panel transition-colors hover:border-line-strong',
            t.id === chosen ? 'border-accent ring-1 ring-accent' : 'border-line')}>
          <span className="text-sm font-semibold">{t.name}</span>
          <span className="text-sm text-muted">{t.summary}</span>
          <span className="flex flex-wrap gap-1">{t.systems.map((s) => <Tag key={s}>{s}</Tag>)}</span>
        </button>
      ))}
    </div>
  );
}
