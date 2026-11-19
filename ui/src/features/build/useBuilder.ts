// The builder page's state: the templates, the chosen one's form and the draft of choices, the preview (asked for on
// People and Review, whenever the choices change), the build, and the built world polled until it is prepared.
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError } from '@/api/client';
import { buildWorld, prepareWorld, previewWorld, templateForm, templates as fetchTemplates, worldState } from '@/api/worlds';
import { useAction } from '@/hooks/useAction';
import { pollWhile, useQuery } from '@/hooks/useQuery';
import type { Preview, Problem, TemplateForm } from '@/model/world';
import { choicesOf, type Draft, draftFor, prune, type Step } from './builder';

const NONE: Draft = { template: null, company: '', size: 0, seed: 1, systems: [], options: {}, counts: {}, edits: {}, added: [], removed: [], events: {} };

/** The chosen template's form, and the draft reset to its defaults whenever another template is chosen. */
function useDraft() {
  const list = useQuery('templates', fetchTemplates);
  const [draft, setDraft] = useState<Draft>(NONE);
  const id = draft.template ?? list.data?.[0]?.id ?? null;
  const formQ = useQuery(id ? `template:${id}` : null, () => templateForm(id ?? ''));
  const form = formQ.data && formQ.data.id === id ? formQ.data : null;
  useEffect(() => { if (form) setDraft((d) => (d.template === form.id && d.company ? d : draftFor(form))); }, [form]);
  const shown = form && draft.template === form.id ? draft : { ...NONE, template: id };
  return { templates: list.data ?? [], form, draft: shown, setDraft, error: list.error ?? formQ.error };
}

/** The preview of the draft's choices, asked for (debounced) on `active` steps; a preview of other choices reads as none. */
function usePreview(form: TemplateForm | null, draft: Draft, active: boolean, setDraft: (f: (d: Draft) => Draft) => void) {
  const [got, setGot] = useState<{ key: string; preview: Preview } | null>(null);
  const key = JSON.stringify(choicesOf(draft));
  useEffect(() => {
    if (!form || !active) return;
    let live = true;
    const t = setTimeout(async () => {
      const preview = await previewWorld(form.id, choicesOf(draft)).catch(() => null);
      if (!live || !preview) return;
      setGot({ key, preview });
      setDraft((d) => { const p = prune(d, preview.people); return p.removed.length === d.removed.length && Object.keys(p.edits).length === Object.keys(d.edits).length ? d : p; });
    }, 250);
    return () => { live = false; clearTimeout(t); };
  }, [form, active, key, draft, setDraft]);
  return got?.key === key ? got.preview : null;
}

/** Why a build was refused, in words: its first problem, or the taken name with the free one arenad suggests. */
function refusal(e: unknown): Error {
  if (!(e instanceof ApiError)) return e instanceof Error ? e : new Error(String(e));
  const body = e.body as { problems?: Problem[]; error?: string; suggested?: string } | undefined;
  const said = body?.problems?.[0]?.message ?? body?.error ?? e.message;
  const text = said.charAt(0).toUpperCase() + said.slice(1);
  return new Error(body?.suggested ? `${text} (${body.suggested} is free)` : text);
}

/** The built world, polled every second while it is prepared; `retry` prepares it again after a failure. */
function useBuilt() {
  const [name, setName] = useState<string | null>(null);
  const q = useQuery(name ? `world:${name}` : null, () => worldState(name ?? ''), pollWhile(1000, (w) => w.state === 'preparing'));
  const retry = useCallback(async () => { if (name) { await prepareWorld(name); q.reload(); } }, [name, q]);
  return { name, setName, world: q.data, retry };
}

export function useBuilder() {
  const { templates, form, draft, setDraft, error } = useDraft();
  const [step, setStep] = useState<Step>('template');
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
  return { templates, form, draft, setDraft, step, setStep, preview, error, build, built };
}
