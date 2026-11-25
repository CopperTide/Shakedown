// The world builder (`/api/templates`) and the benchmarks to choose between (`/api/worlds`).
import type { BuiltWorld, Choices, Preview, TemplateForm, TemplateSummary } from '@/model/world';
import { tracked } from '@/lib/productAnalytics';
import { get, path, post } from './client';

export const templates = () => get<TemplateSummary[]>('/api/templates');
export const templateForm = (id: string) => get<TemplateForm>(path`/api/templates/${id}`);
export const previewWorld = (id: string, c: Choices) => post<Preview>(path`/api/templates/${id}/preview`, c);
/** 202 `{name, state}`; a refusal is an `ApiError` whose `body` holds `problems` (422) or `suggested` (409). */
export const buildWorld = (template: string, c: Choices) =>
  post<{ name: string; state: BuiltWorld['state'] }>('/api/worlds', { template, choices: c }).then(tracked('world created'));
export const worlds = () => get<BuiltWorld[]>('/api/worlds');
export const worldState = (name: string) => get<BuiltWorld>(path`/api/worlds/${name}`);
export const chooseWorld = (name: string) => post<BuiltWorld>(path`/api/worlds/${name}/use`, {});
export const prepareWorld = (name: string) => post<{ name: string; state: BuiltWorld['state'] }>(path`/api/worlds/${name}/prepare`, {});
