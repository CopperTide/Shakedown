import type { ReactNode } from 'react';
import { CodeBlock } from '@/components/ui/code-block';
import { Panel, PanelHeader } from '@/components/ui/panel';
import type { Preview, TemplateForm } from '@/model/world';
import { type Draft, keptRoles, memoryLine } from './builder';
import { plural } from '@/lib/plural';


function Row({ term, children }: { term: string; children: ReactNode }) {
  return <><dt className="text-xs text-muted">{term}</dt><dd className="text-sm">{children}</dd></>;
}

/** The company: its people, systems (with their options), hosts and events. */
function Company({ form, d, p }: { form: TemplateForm; d: Draft; p: Preview }) {
  const systems = form.systems.filter((s) => d.systems.includes(s.id))
    .map((s) => s.label + (s.options.length ? ` (${s.options.map((o) => d.options[s.id]?.[o.name] ?? o.default).join(', ')})` : ''));
  const on = p.events.filter((e) => e.on);
  const traps = on.filter((e) => e.kind === 'trap').length;
  return (
    <Panel>
      <PanelHeader title={p.title} meta={form.name} />
      <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-2 p-4">
        <Row term={form.size.label}>{`${plural(p.people.length, 'simulated person', 'simulated people')} in ${plural(keptRoles(form, d).length, 'role')}`}</Row>
        <Row term="Systems">{systems.join(', ')}</Row>
        {p.hosts.length > 0 && <Row term="Hosts"><span className="font-mono text-xs">{p.hosts.join(' · ')}</span></Row>}
        <Row term="Events">{on.length ? `${plural(traps, 'trap')}, ${plural(on.length - traps, 'event')}` : 'None'}</Row>
      </dl>
    </Panel>
  );
}

function Tasks({ p }: { p: Preview }) {
  return (
    <Panel>
      <PanelHeader title="Tasks" meta={p.tasks.length} />
      <ul className="divide-y divide-line">
        {p.tasks.map((t) => <li key={t.id} className="flex items-baseline gap-3 px-4 py-2.5 text-sm"><span className="min-w-0 flex-1">{t.reason}</span><span className="text-xs text-muted tabular-nums">{plural(t.checks, 'check')}</span></li>)}
      </ul>
    </Panel>
  );
}

/** The images the world needs here (✓ present, ⚠ missing with the command that fetches or builds it), and its memory. */
function Machine({ form, d, p }: { form: TemplateForm; d: Draft; p: Preview }) {
  const present = p.needs.images.filter((i) => i.present).length;
  return (
    <Panel>
      <PanelHeader title="On this machine" />
      <div className="flex flex-col gap-2 p-4 text-sm">
        {present > 0 && <div><span className="text-muted">✓</span> {plural(present, 'image')} present</div>}
        {p.needs.images.filter((i) => !i.present).map((i) => (
          <div key={i.image} className="flex flex-col gap-1">
            <span className="font-medium text-warn">{`⚠ ${i.image}`} <span className="font-normal text-muted">missing</span></span>
            {i.hint && <CodeBlock>{i.hint}</CodeBlock>}
          </div>
        ))}
        <div className="text-xs text-muted">{memoryLine(form, d)}</div>
      </div>
    </Panel>
  );
}

/** Everything the world will be before it is built, and what was left out (muted); why it couldn't be checked. */
export default function ReviewStep({ form, d, preview, error = null }: { form: TemplateForm; d: Draft; preview: Preview | null; error?: string | null }) {
  if (!preview && error) return <Panel><div className="p-4 text-sm text-alarm">{`Can't check the world: ${error}`}</div></Panel>;
  if (!preview) return <Panel><div className="p-4 text-sm text-muted">Checking the world…</div></Panel>;
  return (
    <>
      <div className="grid gap-4 md:grid-cols-2">
        <Company form={form} d={d} p={preview} />
        <Tasks p={preview} />
        <Machine form={form} d={d} p={preview} />
      </div>
      {preview.dropped.length > 0 && (
        <details className="text-xs text-muted">
          <summary className="cursor-pointer">Left out</summary>
          <ul className="mt-1.5 flex flex-col gap-0.5">{preview.dropped.map((x) => <li key={x.what}>{`${x.what}: ${x.why}`}</li>)}</ul>
        </details>
      )}
    </>
  );
}
