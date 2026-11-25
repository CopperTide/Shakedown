import { type AgentKind, type AgentSpec, DEFAULT_OPENROUTER_MODEL, DEFAULT_PROVIDER, type ProviderKind, type ProviderStatus, type Team } from './types';

/** The cost source of a local server's calls (`providers/mod.rs` `LOCAL_COST_SOURCE`). */
export const LOCAL_COST_SOURCE = 'local (no API charge)';

export const MODE_LABEL: Record<AgentKind, string> = { claude_code: 'Claude Code', codex: 'Codex', open: 'API mode', external: 'External', http: 'HTTP agent' };

/** A Bedrock provider (`bedrock:<region>`): shaked calls it with the machine's AWS credentials, and lists it nowhere. */
export const isBedrock = (id: string | null | undefined) => !!id?.trim().startsWith('bedrock:');

/** `bedrock:us-east-1` → `us-east-1`. */
export const bedrockRegion = (id: string) => id.trim().slice('bedrock:'.length);

/** `https://agent.example.com/shakedown` → `agent.example.com`; the text itself when it isn't a URL. */
export function hostOf(url: string | null | undefined): string {
  const t = url?.trim() ?? '';
  try {
    return new URL(t).host || t;
  } catch {
    return t;
  }
}

/** The provider id `p` stands for: itself trimmed, else the default (`openrouter`). */
export function providerId(p: string | null | undefined): string {
  const t = p?.trim();
  return t ? t : DEFAULT_PROVIDER;
}

/** The provider an agent's model calls go to. */
export const providerOf = (a: { provider?: string | null }) => providerId(a.provider);

export function kindOf(id: string): ProviderKind {
  if (id === 'openrouter' || id === 'openai' || id === 'anthropic') return id;
  return 'local';
}

/** A short name: OpenRouter, OpenAI, Anthropic, `<name> key` for a saved key named `key:<name>`, or `<name> (local)`. */
export function providerName(id: string): string {
  if (isBedrock(id)) return 'Bedrock';
  if (id.startsWith('key:')) return `${id.slice('key:'.length)} key`;
  switch (kindOf(id)) {
    case 'openrouter': return 'OpenRouter';
    case 'openai': return 'OpenAI';
    case 'anthropic': return 'Anthropic';
    default: return `${id.replace(/^local:/, '')} (local)`;
  }
}

/** Only OpenRouter has a default model; the others need one named. */
export const hasDefaultModel = (id: string) => !isBedrock(id) && kindOf(id) === 'openrouter';

/** OpenAI and Anthropic report no price, and shaked keeps no price table. */
export const costIsUnknown = (id: string) => kindOf(id) === 'openai' || kindOf(id) === 'anthropic';

/** The providers of a team's open agents. */
export const teamProviders = (team: Team | null | undefined): string[] =>
  [...new Set((team?.agents ?? []).filter((a) => a.kind === 'open').map(providerOf))];

/** "sonnet" → "Sonnet"; a full model id stays as it is. */
export const modelWord = (m: string) => (/^[a-z]+$/.test(m) ? m[0].toUpperCase() + m.slice(1) : m);

type Labelled = Pick<AgentSpec, 'kind' | 'model' | 'provider'> & Partial<Pick<AgentSpec, 'url'>>;

/** The model an agent runs on, in words: "Sonnet" (Claude Code's default when none is named), "Default model", "deepseek/… (default)"; an HTTP agent's host. */
export function modelOf(a: Labelled): string {
  const m = a.model?.trim();
  if (a.kind === 'http') return hostOf(a.url) || 'Its own model';
  if (a.kind === 'open') return m || (hasDefaultModel(providerOf(a)) ? `${DEFAULT_OPENROUTER_MODEL} (default)` : 'No model');
  // shaked starts Claude Code with Sonnet when the team names no model.
  if (!m) return a.kind === 'claude_code' ? 'Sonnet' : a.kind === 'external' ? 'Its own model' : 'Default model';
  return modelWord(m);
}

/** Where an agent runs: its CLI, its API provider, or the external system. */
export const runsOn = (a: Pick<AgentSpec, 'kind' | 'provider'>) => (a.kind === 'open' ? providerName(providerOf(a)) : MODE_LABEL[a.kind]);

/** "Claude Code · Haiku", "OpenRouter · deepseek/… (default)": where an agent runs and on which model. Every agent label uses it. */
export const agentModelLine = (a: Labelled) => `${runsOn(a)} · ${modelOf(a)}`;

/** More than the line says, for its tooltip: a Bedrock agent's region, an HTTP agent's whole URL. */
export function agentModelTip(a: Labelled): string | undefined {
  if (a.kind === 'http') return a.url?.trim() || undefined;
  if (a.kind === 'open' && isBedrock(a.provider)) return `Bedrock in ${bedrockRegion(providerOf(a))}`;
  return undefined;
}

export type ProviderState = 'ready' | 'unavailable' | 'not checked';

/** Ready, unavailable, or (a local server /api/health didn't ask) not checked. */
export function providerState(s: ProviderStatus): ProviderState {
  if (s.available) return 'ready';
  return s.kind === 'local' && !s.checked ? 'not checked' : 'unavailable';
}

const GLYPH: Record<ProviderState, string> = { ready: '✓', unavailable: '✕', 'not checked': '?' };
export const stateGlyph = (st: ProviderState) => GLYPH[st];

/** The reason shown next to the state (none for "not checked": the word says it). */
export const stateReason = (s: ProviderStatus): string | null =>
  providerState(s) === 'not checked' ? null : s.why ?? null;

/** The status bar's one line about API mode: how many providers are ready, and the shared reason when none is. */
export function providerSummary(list: ProviderStatus[]): { glyph: string; text: string } | null {
  if (!list.length) return null;
  const ready = list.filter((s) => providerState(s) === 'ready').length;
  const unchecked = list.filter((s) => providerState(s) === 'not checked').length;
  const count = `${ready} of ${list.length} ready`;
  if (ready === list.length) return { glyph: GLYPH.ready, text: count };
  if (ready > 0) return { glyph: GLYPH.ready, text: unchecked ? `${count}, ${unchecked} not checked` : count };
  if (unchecked) return { glyph: GLYPH['not checked'], text: `${count}, ${unchecked} not checked` };
  return { glyph: GLYPH.unavailable, text: `${count}: ${sharedReason(list)}` };
}

/** "no API key set" when every provider lacks its key, the one reason they share, else a pointer to the list. */
function sharedReason(list: ProviderStatus[]): string {
  const why = list.map((s) => s.why ?? '');
  if (why.every((w) => /_API_KEY is not set$/.test(w))) return 'no API key set';
  return why.every((w) => w === why[0]) && why[0] ? why[0] : 'see each provider';
}

/** An option of a provider select: unavailable ones are disabled and say why. */
export interface ProviderOption {
  id: string;
  text: string;
  disabled: boolean;
}

/**
 * The providers to choose from. `current` (the agent's provider) is always listed and never
 * disabled, so a team keeps showing what it names; one the shakedown doesn't know is "not configured".
 */
export function providerOptions(list: ProviderStatus[], current: string): ProviderOption[] {
  const out: ProviderOption[] = list.map((s) => {
    const st = providerState(s);
    const why = stateReason(s);
    const text = st === 'ready' ? s.label : `${s.label} (${st}${why ? `: ${why}` : ''})`;
    return { id: s.id, text, disabled: st === 'unavailable' && s.id !== current };
  });
  if (!list.some((s) => s.id === current)) {
    const text = isBedrock(current) ? `Bedrock (${bedrockRegion(current)})` : list.length ? `${current} (not configured)` : providerName(current);
    out.push({ id: current, text, disabled: false });
  }
  return out;
}

/** Why an open agent on `provider` with `model` can't run, if the UI can tell. */
export function modelProblem(provider: string, model: string | null | undefined, list: ProviderStatus[]): string | null {
  // Bedrock is listed nowhere: only a model is needed (Connect your agent tested the rest).
  if (isBedrock(provider)) return model?.trim() ? null : 'Name a model for Bedrock';
  return listedModelProblem(provider, model, list);
}

function listedModelProblem(provider: string, model: string | null | undefined, list: ProviderStatus[]): string | null {
  const s = list.find((x) => x.id === provider);
  if (!s && list.length) return `${provider} is not configured on this Shakedown core`;
  if (s && providerState(s) === 'unavailable') return `${s.label} is unavailable${s.why ? `: ${s.why}` : ''}`;
  if (!model?.trim() && !hasDefaultModel(provider)) return `Name a model for ${s?.label ?? providerName(provider)}`;
  return null;
}

/** The model box's hint: the default, or that a model must be named. */
export const modelPlaceholder = (provider: string) =>
  hasDefaultModel(provider) ? `${DEFAULT_OPENROUTER_MODEL} (default)` : 'model id (required)';
