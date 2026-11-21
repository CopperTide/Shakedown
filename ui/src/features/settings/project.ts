// The project settings form, and the settings it writes. Pure, so the checks are tested alone.
import type { EditPolicy, ProjectSettings } from '@/model/project';

export interface SettingsForm {
  name: string;
  model: string;
  provider: string;
  edit: EditPolicy;
  spendMode: 'always_ask' | 'under_cap';
  cap: string;
}

export function formFrom(s: ProjectSettings): SettingsForm {
  const spend = s.permissions?.spend;
  const cap = spend && typeof spend === 'object' ? String(spend.under_cap.usd_per_day) : '';
  return {
    name: s.name,
    model: s.assistant?.model ?? '',
    provider: s.assistant?.provider ?? '',
    edit: s.permissions?.edit ?? 'diff_first',
    spendMode: cap ? 'under_cap' : 'always_ask',
    cap,
  };
}

/** `base` with the form applied; throws a message a person can act on. */
export function settingsFrom(base: ProjectSettings, f: SettingsForm): ProjectSettings {
  const cap = Number(f.cap);
  if (f.spendMode === 'under_cap' && !(f.cap.trim() && Number.isFinite(cap) && cap > 0)) {
    throw new Error('The daily cap must be a positive number of US dollars.');
  }
  const spend = f.spendMode === 'under_cap' ? { under_cap: { usd_per_day: cap } } : 'always_ask';
  const next: ProjectSettings = { ...base, name: f.name, permissions: { edit: f.edit, spend } };
  if (f.model.trim()) next.assistant = { model: f.model.trim(), ...(f.provider.trim() ? { provider: f.provider.trim() } : {}) };
  else delete next.assistant;
  return next;
}
