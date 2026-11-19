// The builder page's state: the templates, the chosen one's form and the draft of choices, the preview (asked for on
// People and Review, whenever the choices change), the build, and the built world polled until it is prepared.
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError } from '@/api/client';
import { buildWorld, prepareWorld, previewWorld, templateForm, templates as fetchTemplates, worldState } from '@/api/worlds';
import { useAction } from '@/hooks/useAction';
import { pollWhile, useQuery } from '@/hooks/useQuery';
import { errorMessage } from '@/lib/errors';
import type { Person, Preview, Problem, TemplateForm } from '@/model/world';
import { afterPreview, blankDraft, choicesOf, type Draft, draftFor, type Step } from './builder';

/** The chosen template's form, and the draft reset to its defaults whenever another template is chosen. */
function useDraft() {
  const list = useQuery('templates', fetchTemplates);
  const [draft, setDraft] = useState<Draft>(() => blankDraft(null));
  const id = draft.template ?? list.data?.[0]?.id ?? null;
  const formQ = useQuery(id ? `template:${id}` : null, () => templateForm(id ?? ''));
  const form = formQ.data && formQ.data.id === id ? formQ.data : null;
  useEffect(() => { if (form) setDraft((d) => (d.template === form.id && d.company ? d : draftFor(form))); }, [form]);
  const shown = form && draft.template === form.id ? draft : blankDraft(id);
  return { templates: list.data ?? [], form, draft: shown, setDraft, error: list.error ?? formQ.error };
}

type Landed = { template: string; key: string } & ({ preview: Preview; error?: undefined } | { preview?: undefined; error: string });

/**
 * The preview of the draft's choices, asked for (debounced) on `active` steps: `preview` and `error` are of these
 * choices only (none while the next is asked for), `people` the last drawn for this template, kept meanwhile so that
 * the names being edited stay on screen.
 */
function usePreview(form: TemplateForm | null, draft: Draft, active: boolean, setDraft: (f: (d: Draft) => Draft) => void) {
  const [got, setGot] = useState<Landed | null>(null);
  const [people, setPeople] = useState<{ template: string; people: Person[] } | null>(null);
  const key = JSON.stringify(choicesOf(draft));
  useEffect(() => {
    if (!form || !active) return;
    let live = true;
    const t = setTimeout(async () => {
      const at = { template: form.id, key };
      try {
        const preview = await previewWorld(form.id, choicesOf(draft));
        if (!live) return;
        setGot({ ...at, preview });
        setPeople({ template: form.id, people: preview.people });
        setDraft((d) => afterPreview(d, preview));
      } catch (e) {
        if (live) setGot({ ...at, error: errorMessage(e) });
      }
    }, 250);
    return () => { live = false; clearTimeout(t); };
  }, [form, active, key, draft, setDraft]);
  const mine = got?.key === key && got.template === form?.id ? got : null;
  return { preview: mine?.preview ?? null, error: mine?.error ?? null, people: people && people.template === form?.id ? people.people : null };
}

/** Why a build was refused, in words: its first problem, or the taken name with the free one arenad suggests. */
function refusal(e: unknown): Error {
  if (!(e instanceof ApiError)) return e instanceof Error ? e : new Error(String(e));
  const body = e.body as { problems?: Problem[]; error?: string; suggested?: string } | undefined;
  const said = body?.problems?.[0]?.message ?? body?.error ?? e.message;
  const text = said.charAt(0).toUpperCase() + said.slice(1);
  return new Error(body?.suggested ? `${text} (${body.suggested} is free)` : text);
}

/**
 * The built world, polled every second while it is prepared; `retry` prepares it again after a failure (or when it
 * was never prepared): a preparation already under way (409) is waited for, any other refusal is its `error`.
 */
function useBuilt() {
  const [name, setName] = useState<string | null>(null);
  const q = useQuery(name ? `world:${name}` : null, () => worldState(name ?? ''), pollWhile(1000, (w) => w.state === 'preparing'));
  const retry = useAction(async () => {
    if (!name) return;
    await prepareWorld(name).catch((e: unknown) => { if (!(e instanceof ApiError && e.status === 409)) throw e; });
    q.reload();
  });
  return { name, setName, world: q.data, retry };
}

export function useBuilder() {
  const { templates, form, draft, setDraft, error } = useDraft();
  const [step, setStepOnly] = useState<Step>('template');
  const preview = usePreview(form, draft, step === 'people' || step === 'review', setDraft);
  const built = useBuilt();
  const latest = useRef({ form, draft });
  latest.current = { form, draft };
  const build = useAction(async () => {
    const { form: f, draft: d } = latest.current;
    if (!f) return;
    const r = await buildWorld(f.id, choicesOf(d)).catch((e: unknown) => { throw refusal(e); });
    built.setName(r.name);
  });
  const { reset } = build;
  /** Another step: a refused build's reason was of the step it was asked from. */
  const setStep = useCallback((s: Step) => { reset(); setStepOnly(s); }, [reset]);
  return { templates, form, draft, setDraft, step, setStep, preview: preview.preview, previewError: preview.error, people: preview.people, error, build, built };
}
