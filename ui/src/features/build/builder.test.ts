import { describe, expect, it } from 'vitest';
import { afterPreview, blankDraft, choicesOf, chooseTemplate, draftFor, memoryLine, prune, roleCount, STEP_LABEL, STEPS, stepProblem, toggleSystem } from './builder';
import { form, people, preview } from './fixtures';

describe('the builder draft', () => {
  it('starts from the template defaults', () => {
    expect(draftFor(form)).toMatchObject({ template: 'example', company: 'Example Co', size: 10, systems: ['code', 'tracker'], counts: {} });
  });
  it('choosing chat adds what it requires and dropping tracker drops chat', () => {
    const d = toggleSystem(draftFor(form), form, 'chat', true);
    expect(d.systems).toEqual(['code', 'tracker', 'chat']);
    expect(toggleSystem(d, form, 'tracker', false).systems).toEqual(['code']);
    expect(toggleSystem(d, form, 'code', false).systems).toContain('code');
  });
  it('choosing chat without tracker brings tracker back', () => {
    const d = toggleSystem(draftFor(form), form, 'tracker', false);
    expect(toggleSystem(d, form, 'chat', true).systems).toEqual(['code', 'tracker', 'chat']);
  });
  it('counts follow size until set by hand, rounded half up and never under the minimum', () => {
    expect(roleCount(form, { ...draftFor(form), size: 40 }, 'dev')).toBe(30);
    expect(roleCount(form, { ...draftFor(form), size: 10 }, 'dev')).toBe(8);
    expect(roleCount(form, { ...draftFor(form), size: 1 }, 'dev')).toBe(1);
    expect(roleCount(form, { ...draftFor(form), size: 40, counts: { dev: 12 } }, 'dev')).toBe(12);
    expect(roleCount(form, { ...draftFor(form), size: 40 }, 'pm')).toBe(1);
  });
  it('prunes edits and removals of people no longer generated', () => {
    const d = { ...draftFor(form), edits: { 'dev-012': { name: 'Ana Ruiz' }, 'dev-003': { name: 'Bo Lee' } }, removed: ['dev-011', 'dev-002'] };
    expect(prune(d, people(10))).toMatchObject({ edits: { 'dev-003': { name: 'Bo Lee' } }, removed: ['dev-002'] });
  });
  it('says what blocks each step', () => {
    expect(stepProblem({ ...draftFor(form), template: null }, form, 'template', null)).toBe('Choose a template');
    expect(stepProblem({ ...draftFor(form), company: '  ' }, form, 'company', null)).toBe('Give the company a name');
    expect(stepProblem({ ...draftFor(form), size: 99 }, form, 'company', null)).toBe('Size is between 2 and 40');
    expect(stepProblem(draftFor(form), form, 'systems', null)).toBeNull();
    expect(stepProblem({ ...draftFor(form), counts: { dev: 0 } }, form, 'people', null)).toBe('Developers need at least 1');
    expect(stepProblem({ ...draftFor(form), counts: { pm: 0 }, systems: ['code'] }, form, 'people', null)).toBeNull();
  });
  it('review is blocked by the first preview problem, and while there is no preview', () => {
    const problems = [{ path: 'needs.images.toy:1', message: 'toy:1 is missing: make toy' }, { path: 'x', message: 'second' }];
    expect(stepProblem(draftFor(form), form, 'review', { ...preview, problems })).toBe('toy:1 is missing: make toy');
    expect(stepProblem(draftFor(form), form, 'review', preview)).toBeNull();
    expect(stepProblem(draftFor(form), form, 'review', null)).toBe('Checking the world…');
  });
  it('sends only what the user chose', () => {
    expect(choicesOf(draftFor(form))).toEqual({ seed: 1, company: { name: 'Example Co' }, size: 10, systems: ['code', 'tracker'], options: {}, people: { counts: {}, edits: {}, added: [], removed: [] }, events: {} });
  });
  it('trims the company name it sends', () => {
    expect(choicesOf({ ...draftFor(form), company: ' Acme ' }).company).toEqual({ name: 'Acme' });
  });
  it('says the memory of the chosen systems in GB, or MB under 1 GB', () => {
    expect(memoryLine(form, draftFor(form))).toBe('About 2.1 GB of memory');
    expect(memoryLine(form, { ...draftFor(form), systems: ['code', 'tracker', 'chat'] })).toBe('About 2.8 GB of memory');
    expect(memoryLine(form, { ...draftFor(form), systems: ['code'] })).toBe('About 900 MB of memory');
  });
  it('names its six steps in order', () => {
    expect(STEPS.map((s) => STEP_LABEL[s])).toEqual(['Template', 'Company', 'Systems', 'People', 'Events', 'Review']);
  });
});

describe('review', () => {
  it('is blocked by a missing image even when the preview names no problem', () => {
    const missing = { ...preview, needs: { images: [{ image: 'toy:1', present: false, hint: 'make toy' }], memory_mb: 90 } };
    expect(stepProblem(draftFor(form), form, 'review', missing)).toBe('One image is missing: build it, then build the world');
  });
});

describe('the builder after a preview', () => {
  const edited = { ...draftFor(form), edits: { 'dev-003': { name: 'Bo Lee' } }, removed: ['dev-002'] };
  it('keeps every edit when the preview made no world (a choice that does not fit)', () => {
    const failed = { ...preview, people: [], tasks: [], problems: [{ path: 'people.counts.dev', message: 'Developers need at least 1' }] };
    expect(afterPreview(edited, failed)).toBe(edited);
    expect(afterPreview(edited, { ...failed, people: people(1) })).toBe(edited);
  });
  it('prunes edits of people a world no longer has, and is the same draft when nothing goes', () => {
    expect(afterPreview(edited, { ...preview, people: people(2) })).toMatchObject({ edits: {}, removed: ['dev-002'] });
    expect(afterPreview(edited, preview)).toBe(edited);
  });
  it('choosing another template starts from a blank draft; the same one keeps it', () => {
    expect(chooseTemplate(edited, 'other')).toEqual(blankDraft('other'));
    expect(chooseTemplate(edited, 'example')).toBe(edited);
  });
});

describe('review copy', () => {
  it('a missing image the builder names reads as the mockup says; another problem comes first', () => {
    const image = { path: 'needs.images.toy:1', message: 'the Docker image toy:1 is missing: make toy' };
    const missing = { ...preview, needs: { images: [{ image: 'toy:1', present: false, hint: 'make toy' }], memory_mb: 90 }, problems: [image] };
    expect(stepProblem(draftFor(form), form, 'review', missing)).toBe('One image is missing: build it, then build the world');
    expect(stepProblem(draftFor(form), form, 'review', { ...missing, problems: [image, { path: 'size', message: 'Size is between 2 and 40' }] })).toBe('Size is between 2 and 40');
  });
});
