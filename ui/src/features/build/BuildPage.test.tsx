import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';
import WorldPicker from '@/features/start/WorldPicker';
import { BuildView } from './BuildPage';
import { BuildingView } from './Building';
import { draftFor } from './builder';
import { described } from './EventsStep';
import { built, form, people, preview, templates, worlds } from './fixtures';

const draft = draftFor(form);
const noop = () => {};
const draw = (p: Partial<ComponentProps<typeof BuildView>> = {}) => renderToStaticMarkup(
  <BuildView form={form} templates={templates} draft={draft} step="template" preview={preview} busy={false} error={null}
    onDraft={noop} onStep={noop} onBuild={noop} {...p} />,
);

describe('Build a world', () => {
  it('shows the steps with the done ones ticked and the current one named', () => {
    const html = draw({ step: 'systems' });
    expect(html).toContain('Build a world');
    expect(html).toMatch(/✓<\/span>\s*Template/);
    expect(html).toMatch(/✓<\/span>\s*Company/);
    expect(html).toContain('aria-current="step"');
    expect(html).toContain('Step 3 of 6 · Systems');
  });
  it('lists the templates with the chosen one marked', () => {
    const html = draw();
    expect(html).toContain('Other example');
    expect(html).toMatch(/aria-pressed="true"[^>]*>.*Example/);
    expect(html).not.toContain('>Back<');
  });
  it('blocks Next with the step\'s problem in words', () => {
    const html = draw({ step: 'company', draft: { ...draft, company: '' } });
    expect(html).toContain('Give the company a name');
    expect(html).toMatch(/disabled=""[^>]*>Next</);
  });
  it('a required system cannot be unticked, and the memory is said', () => {
    const html = draw({ step: 'systems' });
    expect(html).toMatch(/aria-label="Code host"[^>]*disabled=""|disabled=""[^>]*aria-label="Code host"/);
    expect(html).toContain('About 2.1 GB of memory');
  });
  it('people: each role with its count, and the names of the role shown as inputs', () => {
    const html = draw({ step: 'people', preview: { ...preview, people: people(3) } });
    expect(html).toContain('Developers');
    expect(html).toMatch(/aria-label="Developers"[^>]*value="8"|value="8"[^>]*aria-label="Developers"/);
    expect(html).toContain('value="Ada Example"');
    expect(html).toContain('+ Add person');
  });
  it('events: a checkbox, its label (its description filled in on hovering it), Trap or Event, its params inline, and only those its systems allow', () => {
    const html = draw({ step: 'events' });
    expect(html).toContain('A test is flaky');
    expect(html).not.toContain('One test fails');
    expect(described(form.events[0], { on: true, params: { rate: 3 } })).toBe('One test fails 3 in 10 runs');
    expect(html).toContain('>Trap<');
    expect(html).toContain('Fails in 10');
    expect(html).toContain('The tracker goes down');
    expect(draw({ step: 'events', draft: { ...draft, systems: ['code'] } })).not.toContain('The tracker goes down');
  });
  it('review names a missing image with its hint and keeps Build world disabled', () => {
    const html = draw({ step: 'review', preview: { ...preview, needs: { images: [{ image: 'toy:1', present: false, hint: 'make toy' }], memory_mb: 90 } } });
    expect(html).toContain('⚠ toy:1');
    expect(html).toContain('make toy');
    expect(html).toMatch(/disabled=""[^>]*>Build world</);
  });
  it('review shows the tasks with their checks and what was left out', () => {
    const html = draw({ step: 'review' });
    expect(html).toContain('The build is red');
    expect(html).toContain('4 checks');
    expect(html).toContain('Left out');
    expect(html).toMatch(/[^d]>Build world</);
    expect(html).not.toMatch(/disabled=""[^>]*>Build world</);
  });
  it('preparing shows the world, the time so far and a bar', () => {
    const html = renderToStaticMarkup(<BuildingView world={{ ...built, elapsed_ms: 23_000 }} onRun={noop} onExperiment={noop} onRetry={noop} />);
    expect(html).toContain('◐ Preparing · 23 s');
    expect(html).toContain('animate-slide');
  });
  it('a ready world offers Run and New experiment', () => {
    const html = renderToStaticMarkup(<BuildingView world={{ ...built, state: 'ready', elapsed_ms: 38_000 }} onRun={noop} onExperiment={noop} onRetry={noop} />);
    expect(html).toContain('✓ Ready');
    expect(html).toContain('38 s');
    expect(html).toContain('New experiment');
  });
  it('a failed world says why and offers Try again', () => {
    const html = renderToStaticMarkup(<BuildingView world={{ ...built, state: 'failed', error: 'the roster refused its config' }} onRun={noop} onExperiment={noop} onRetry={noop} />);
    expect(html).toContain('✕ Failed');
    expect(html).toContain('the roster refused its config');
    expect(html).toContain('Try again');
  });
});

describe('the world picker', () => {
  it('lists the titles with the current one chosen, and ends with Build a world…', () => {
    const html = renderToStaticMarkup(<MemoryRouter><WorldPicker value="retail" worlds={worlds} onChoose={noop} busy={false} /></MemoryRouter>);
    const options = [...html.matchAll(/<option[^>]*>([^<]*)<\/option>/g)].map((m) => m[1]);
    expect(options).toEqual(['retail', 'Example Co', 'Build a world…']);
    expect(html).toMatch(/<option value="retail" selected="">/);
  });
});
