// A step in one or two plain sentences, for the step drawer: who did what, on which system, and
// how it went (refused or failed, and why). The raw record stays behind "Details".
import { asRecord } from '@/lib/record';
import { type Action, actionWord, classify, kindWord, worldInfo } from '@/model/actions';
import { formatDuration, isWorldActor, reasoningWord } from '@/model/lanes';
import type { Event, Team } from '@/model/types';
import { clockText } from './flow';

export interface StepSummary {
  title: string;
  text: string;
  abnormal: boolean;
}

type Payload = Record<string, unknown>;
const clip = (s: string, n = 180) => (s.length > n ? `${s.slice(0, n - 1).trimEnd()}…` : s);
const quote = (s: unknown) => `“${clip(String(s ?? '').replace(/\s+/g, ' ').trim())}”`;

function nameIn(team: Team, id: string | null): string {
  if (isWorldActor(id)) return 'The world';
  return team.agents.find((a) => a.id === id)?.name ?? id;
}

const VERB = { write: 'wrote to', read: 'read', unknown: 'used' } as const;

/** "It worked, in 120 ms.", "It was refused: …", "It failed: …". */
function outcomeLine(a: Action): string {
  if (a.outcome === 'refused') return `It was refused: ${clip(a.error ?? '')}`;
  if (a.outcome === 'error') return `It failed: ${clip(a.error ?? '')}`;
  return a.durationMs == null ? 'It worked.' : `It worked, in ${formatDuration(a.durationMs)}.`;
}

/** shaked's own refusals of a request through its proxy (its other errors are failures). */
const HTTP_REFUSAL = /is not part of this world|takes HTTPS on port 443 only|not available to this agent|is over 256 MiB/;

/** A request an agent's client sent through the proxy: method, URL, the part serving it, and its status. */
function httpStep(p: Payload, who: string): StepSummary {
  const args = asRecord(p.args);
  const method = String(args.method ?? 'GET');
  const to = typeof p.part === 'string' ? ` to ${p.part}` : '';
  const sent = `${who} sent ${method} ${String(args.url ?? '')}${to}`;
  const title = `${who} · HTTP ${method}`;
  if (p.error == null) return { title, text: `${sent}: ${String(asRecord(p.result).status ?? '')}.`, abnormal: false };
  const how = HTTP_REFUSAL.test(String(p.error)) ? 'It was refused' : 'It failed';
  return { title, text: `${sent}. ${how}: ${clip(String(p.error))}`, abnormal: true };
}

function toolStep(e: Event, events: Event[], who: string): StepSummary {
  if (asRecord(e.payload).name === 'http') return httpStep(asRecord(e.payload), who);
  const a = classify(e, worldInfo(events))!;
  if (a.system?.kind === 'messages') return { title: `${who} · Messages`, text: `${who} read its messages.`, abnormal: false };
  const system = a.system ? `the ${kindWord(a.system)}` : 'a tool';
  const text = `${who} ${VERB[a.effect]} ${system} (${a.tool}). ${outcomeLine(a)}`;
  return { title: `${who} · ${a.system ? actionWord(a) : a.tool}`, text, abnormal: a.outcome !== 'ok' };
}

function modelStep(p: Payload, who: string): StepSummary {
  const model = asRecord(p.request).model ?? p.model;
  const usage = asRecord(p.usage);
  const tin = usage.prompt_tokens ?? p.input_tokens;
  const tout = usage.completion_tokens ?? p.output_tokens;
  const tokens = typeof tin === 'number' && typeof tout === 'number' ? ` ${tin.toLocaleString()} tokens in, ${tout.toLocaleString()} out.` : '';
  return { title: `${who} · Model call`, text: `${who} asked its model${typeof model === 'string' && model ? ` (${model})` : ''}.${tokens}`, abnormal: false };
}

function checkStep(p: Payload): StepSummary {
  const checks = Array.isArray(p.checks) ? (p.checks as { passed: boolean }[]) : [];
  const ok = checks.filter((c) => c.passed).length;
  const text = checks.length ? `The run was scored: ${ok} of ${checks.length} checks passed.` : `The run was scored: it ${p.passed ? 'matches' : 'does not match'} the expected result.`;
  return { title: p.passed ? 'Check passed' : 'Check failed', text, abnormal: !p.passed };
}

type Words = (p: Payload, who: string, to: (v: unknown) => string, e: Event, events: Event[]) => StepSummary;

function endStep(p: Payload, who: string): StepSummary {
  const took = typeof p.seconds === 'number' ? ` after ${clockText(p.seconds)}` : '';
  return p.ok === false
    ? { title: `${who} · Failed`, text: `${who} stopped with an error${took}: ${clip(String(p.error ?? 'no reason given'))}`, abnormal: true }
    : { title: `${who} · Done`, text: `${who} finished${took}.`, abnormal: false };
}

/** Why a dispatch run woke the agent, in a sentence. */
function wakeStep(p: Payload, who: string): StepSummary {
  const mail = Array.isArray(p.mail) ? p.mail.length : 0;
  const why = {
    start: `${who} started work with the ticket.`,
    idle: `Every agent was waiting and the run was not finished, so ${who} was woken to follow up (wake ${String(p.idle_wake ?? '')}).`,
    resume: `${who} carried on from the fork point with its recorded history.`,
  }[String(p.reason)] ?? `${who} was woken by ${mail > 1 ? `${mail} messages` : 'new work or a message'}.`;
  return { title: `${who} · Woken`, text: why, abnormal: false };
}

/** An agent's reasoning: its first 160 characters, and a note when Shakedown cut the record. */
function reasoningStep(p: Payload, who: string): StepSummary {
  const text = clip(String(p.text ?? '').replace(/\s+/g, ' ').trim(), 160);
  const cut = p.truncated ? ' (cut at 32,768 characters)' : '';
  return { title: `${who} · ${reasoningWord(p.kind)}`, text: text + cut, abnormal: false };
}

/** Why a save point was made, as the run page says it. */
const SAVED_WHY: Record<string, string> = {
  model_call: 'before a model call',
  asked: 'asked by the driver',
  setup: "after the task's setup",
};

const WORDS: Partial<Record<Event['kind'], Words>> = {
  delegation: (p, who, to) => ({ title: `${who} · Delegate`, text: `${who} gave ${to(p.to)} work: ${quote(p.task)}`, abnormal: false }),
  report: (p, who, to) => ({
    title: `${who} · Report`,
    text: p.turn_end ? `${who} ended its turn without reporting; what it said went to ${to(p.to)}: ${quote(p.text)}` : `${who} reported to ${to(p.to)}: ${quote(p.text)}`,
    abnormal: false,
  }),
  wake: wakeStep,
  idle: (p, who) => (p.ok === false
    ? { title: `${who} · Turn failed`, text: `${who}'s turn failed: ${clip(String(p.error ?? ''))}`, abnormal: true }
    : { title: `${who} · Waiting`, text: `${who} ended its turn and waits for work. It said: ${quote(p.text)}`, abnormal: false }),
  finish: (p, who) => ({ title: `${who} · Finish`, text: `${who} finished the run: every agent was stopped and the world scored. ${quote(p.summary)}`, abnormal: false }),
  tool_call: (_p, who, _to, e, events) => toolStep(e, events, who),
  model_call: (p, who) => modelStep(p, who),
  message: (p, who, to) => ({ title: `${who} · Message`, text: `${who} sent a message to ${to(p.to)}: ${quote(p.text)}`, abnormal: false }),
  handoff: (p, who, to) => ({ title: `${who} · Handoff`, text: `${who} left a note for ${to(p.to)}: ${quote(p.note)}`, abnormal: false }),
  agent_start: (_p, who) => ({ title: `${who} · Start`, text: `${who} started.`, abnormal: false }),
  agent_end: endStep,
  check: (p) => checkStep(p),
  world_event: (p) => ({ title: `World · ${String(p.kind ?? 'event')}`, text: String(p.text ?? 'Something happened in the world.'), abnormal: false }),
  pause: () => ({ title: 'Paused', text: 'The run was paused: no agent could act until it resumed.', abnormal: false }),
  resume: (p) => ({ title: 'Resumed', text: `The run went on after ${clockText(Number(p.paused_ms ?? 0) / 1000)} paused.`, abnormal: false }),
  reasoning: reasoningStep,
  save_point: (p) => ({ title: 'Saved', text: `The world was saved (${SAVED_WHY[String(p.reason)] ?? SAVED_WHY.model_call}).`, abnormal: p.error != null }),
  run_start: () => ({ title: 'Start', text: 'The run started and the task was set up in the world.', abnormal: false }),
};

/** The step `e` of a branch with `events`, run by `team`, in words. */
export function stepSummary(e: Event, events: Event[], team: Team): StepSummary {
  const to = (v: unknown) => (v == null ? 'the agents after it' : v === '*' ? 'everyone' : nameIn(team, String(v)));
  const words = WORDS[e.kind];
  return words ? words(asRecord(e.payload), nameIn(team, e.agent), to, e, events) : { title: 'World view', text: "The world's view after a step.", abnormal: false };
}
