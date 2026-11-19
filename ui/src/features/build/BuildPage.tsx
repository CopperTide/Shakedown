import { useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { PageBody, PageHeader } from '@/components/ui/page';
import { ErrorState, LoadingState } from '@/components/ui/state';
import { Panel } from '@/components/ui/panel';
import type { Person, Preview, TemplateForm, TemplateSummary } from '@/model/world';
import { BuildingView } from './Building';
import { chooseTemplate, type Draft, keptRoles, memoryLine, roleCount, STEPS, type Step, stepProblem } from './builder';
import CompanyStep from './CompanyStep';
import EventsStep, { eventChoice, offeredEvents } from './EventsStep';
import PeopleStep from './PeopleStep';
import ReviewStep from './ReviewStep';
import Stepper from './Stepper';
import SystemsStep from './SystemsStep';
import TemplateStep from './TemplateStep';
import { useBuilder } from './useBuilder';

interface ViewProps {
  form: TemplateForm | null; templates: TemplateSummary[]; draft: Draft; step: Step; preview: Preview | null;
  busy: boolean; error: string | null; onDraft: (d: Draft) => void; onStep: (s: Step) => void; onBuild: () => void;
  /** The people People shows: the last drawn, kept while the next preview is asked for (default: the preview's). */
  people?: Person[] | null;
  /** Why the preview of these choices failed. */
  previewError?: string | null;
}

/** What the bar says of the step when nothing blocks it: "Example Co · 10 staff", "3 systems · about 2.1 GB", … */
function summary(p: ViewProps, form: TemplateForm): string {
  const d = p.draft;
  const on = offeredEvents(form, d).filter((e) => eventChoice(d, e).on);
  const traps = on.filter((e) => e.kind === 'trap').length;
  const lines: Record<Step, string> = {
    template: form.name,
    company: `${d.company.trim()} · ${d.size} ${form.size.label.toLowerCase()}`,
    systems: `${d.systems.length} systems · ${memoryLine(form, d).replace('About', 'about').replace(' of memory', '')}`,
    people: `${keptRoles(form, d).reduce((n, r) => n + roleCount(form, d, r.id), 0)} people`,
    events: `${traps} trap${traps === 1 ? '' : 's'}, ${on.length - traps} event${on.length - traps === 1 ? '' : 's'}`,
    review: 'Ready to build',
  };
  return lines[p.step];
}

/** Back, what the step comes to or what blocks it (alarm ink), and Next (Build world on Review). */
function Bar(p: ViewProps) {
  const at = STEPS.indexOf(p.step);
  const last = at === STEPS.length - 1;
  const failed = p.step === 'review' && !p.preview && p.previewError ? `Can't check the world: ${p.previewError}` : null;
  const problem = p.error ?? failed ?? stepProblem(p.draft, p.form, p.step, p.preview);
  const next = () => (last ? p.onBuild() : p.onStep(STEPS[at + 1]));
  return (
    <div className="sticky bottom-0 -mx-4 flex flex-wrap items-center gap-3 border-t border-line bg-bg/90 px-4 py-3 backdrop-blur sm:-mx-6 sm:px-6">
      {at > 0 && <Button variant="ghost" onClick={() => p.onStep(STEPS[at - 1])}>Back</Button>}
      {problem
        ? <span role="status" className="min-w-0 text-sm font-medium text-alarm">{`⚠ ${problem}`}</span>
        : p.form && <span className="min-w-0 text-sm text-muted">{summary(p, p.form)}</span>}
      <Button variant="primary" className="ml-auto h-9 px-5 text-sm" disabled={!!stepProblem(p.draft, p.form, p.step, p.preview) || p.busy} onClick={next}>{last ? (p.busy ? 'Building…' : 'Build world') : 'Next'}</Button>
    </div>
  );
}

/** The step's own panel. */
function StepPanel(p: ViewProps & { form: TemplateForm }) {
  const edit = { form: p.form, d: p.draft, onDraft: p.onDraft };
  switch (p.step) {
    case 'template': return <TemplateStep templates={p.templates} chosen={p.draft.template} onChoose={(id) => p.onDraft(chooseTemplate(p.draft, id))} />;
    case 'company': return <CompanyStep {...edit} />;
    case 'systems': return <SystemsStep {...edit} />;
    case 'people': return <PeopleStep {...edit} people={p.people ?? p.preview?.people ?? null} error={p.previewError ?? null} />;
    case 'events': return <EventsStep {...edit} />;
    case 'review': return <ReviewStep form={p.form} d={p.draft} preview={p.preview} error={p.previewError ?? null} />;
  }
}

/** Build a world: the stepper, the step's panel and the bar under it. */
export function BuildView(p: ViewProps) {
  return (
    <>
      <PageHeader width="broad" title="Build a world" />
      <PageBody width="broad" className="pb-0">
        <Stepper step={p.step} onStep={p.onStep} />
        {p.form || p.step === 'template'
          ? <StepPanel {...p} form={p.form ?? ({ systems: [], roles: [], events: [] } as unknown as TemplateForm)} />
          : <Panel><LoadingState label="Loading the template" /></Panel>}
        <Bar {...p} />
      </PageBody>
    </>
  );
}

/** The builder: its steps, then the world being prepared and, once ready, the way to run it. */
export default function BuildPage() {
  const b = useBuilder();
  const nav = useNavigate();
  if (b.built.name) {
    const world = b.built.world;
    return (
      <>
        <PageHeader width="broad" title="Build a world" />
        <PageBody width="broad">
          {world ? <BuildingView world={world} onRun={() => nav('/run')} onExperiment={() => nav('/experiments/new')} onRetry={() => void b.built.retry.run()} retry={b.built.retry} /> : <Panel><LoadingState label="Preparing" /></Panel>}
        </PageBody>
      </>
    );
  }
  if (b.error && !b.templates.length) return <><PageHeader width="broad" title="Build a world" /><PageBody width="broad"><ErrorState error={b.error} /></PageBody></>;
  return <BuildView form={b.form} templates={b.templates} draft={b.draft} step={b.step} preview={b.preview} busy={b.build.busy} error={b.build.error}
    people={b.people} previewError={b.previewError} onDraft={(d) => b.setDraft(() => d)} onStep={b.setStep} onBuild={() => void b.build.run()} />;
}
