// Runtimes (`/api/runtimes`): the packages that run worlds, installed from a git repo (or a
// folder of it) or an uploaded archive, with their conformance check.
import { del, get, path, post } from './client';
import type { GitImport } from './project';

/** One step of a package's conformance check. */
export interface CheckStep { name: string; ok: boolean; detail: string }

/** A package's conformance check (`shakedown-part-check` for an integration; a benchmark is described). */
export interface RuntimeCheck {
  package: string;
  passed: boolean;
  steps: CheckStep[];
  skipped: string[];
  checked_at?: string;
}

/** An installed package and the project's worlds it runs. */
export interface RuntimePackage {
  name: string;
  kind: 'benchmark' | 'integration' | 'builder';
  version?: string;
  dir?: string;
  used_by: string[];
  /** Installs only: whether this shaked holds it now. */
  registered?: boolean;
  check?: RuntimeCheck | null;
}

/** One install: where it came from and the packages it holds. */
export interface RuntimeInstall {
  id: string;
  source: 'git' | 'archive';
  /** The git URL, or the archive's file name. */
  url: string;
  ref: string;
  /** The folder whose packages were installed (`""`: all); null for an Import from git. */
  path: string | null;
  /** The commit, or the archive's SHA-256. */
  commit: string;
  installed_at: string;
  /** False for an Import from git, which also wrote project files (managed there). */
  runtime: boolean;
  items: number;
  packages: RuntimePackage[];
}

export interface Runtimes {
  installs: RuntimeInstall[];
  /** Packages installed from folders (`SHAKEDOWN_PACKAGES`). */
  builtin: RuntimePackage[];
  /** Hosted Shakedown: installing runs code on the server, so it is refused. */
  hosted: boolean;
}

/** What installing found and does; once installed, the checks too. */
export type RuntimePlan = GitImport & { source: 'git' | 'archive'; packages_only: string | null; checks?: RuntimeCheck[] };

/** Where a runtime comes from. */
export type RuntimeSource =
  | { kind: 'git'; url: string; ref?: string }
  | { kind: 'archive'; name: string; data: string };

export const runtimes = () => get<Runtimes>('/api/runtimes');

/** Installs (or, with an install's `id`, updates) a runtime; without `confirm`, only the plan. */
export function installRuntime(source: RuntimeSource, opts: { path?: string; id?: string }, confirm: boolean) {
  const from = source.kind === 'git'
    ? { url: source.url, ref: source.ref || undefined }
    : { archive: { name: source.name, data: source.data } };
  return post<RuntimePlan>('/api/runtimes', { ...from, path: opts.path || undefined, id: opts.id || undefined, confirm });
}

export const checkRuntime = (id: string) => post<{ id: string; checks: RuntimeCheck[] }>(path`/api/runtimes/${id}/check`, {});
export const removeRuntime = (id: string) => del<{ removed: string; packages: string[] }>(path`/api/runtimes/${id}`);

/** A file's bytes as base64, for an upload. */
export function base64Of(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result).replace(/^data:[^,]*,/, ''));
    r.onerror = () => reject(r.error ?? new Error('the file could not be read'));
    r.readAsDataURL(file);
  });
}
