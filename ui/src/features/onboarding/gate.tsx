// The onboarding gate: until the core steps are done (a project, a model, a world, agents and a
// first scenario, as shaked reports them), every page but those the steps need sends you to Get
// started. The state comes from the server on every navigation while the gate is shut, never from
// the browser.
import { type ReactNode, useEffect, useRef } from 'react';
import { Navigate, useLocation } from 'react-router-dom';
import { type ProjectState, useProject } from '@/hooks/useProject';
import { checkProviders, useCheckedProviders } from '@/hooks/useProviders';
import type { ProviderStatus } from '@/model/types';
import { coreDone, stageOf, stepStates } from './steps';

/** Pages open before the steps are done: Get started itself, model keys, and the editors a step opens. */
export const EXEMPT: RegExp[] = [
  /^\/get-started(\/|$)/,
  /^\/settings\/(models|tokens)(\/|$)/,
  /^\/(worlds|scenarios|setups)\/[^/]+$/,
];

export const exempt = (pathname: string) => EXEMPT.some((r) => r.test(pathname));

export type Gate = 'wait' | 'open' | 'shut';

/**
 * Whether the app is open, from the project and provider answers. A failed read opens it (the
 * page then shows the error) rather than locking everyone out on a hiccup.
 */
export function gateOf(project: ProjectState | null, providers: ProviderStatus[] | null, failed: boolean): Gate {
  if (failed) return 'open';
  if (!project || !providers) return 'wait';
  const stage = stageOf(project.open, project.open ? undefined : project.message);
  const counts = project.open ? project.status.counts : {};
  return coreDone(stepStates({ project: stage, counts, providersOk: providers.some((p) => p.available) })) ? 'open' : 'shut';
}

export default function OnboardingGate({ children }: { children: ReactNode }) {
  const { pathname } = useLocation();
  const project = useProject();
  const providers = useCheckedProviders(true);
  const gate = gateOf(project.data, providers.list, !!(project.error || providers.error));
  const shut = useRef(false);
  shut.current = gate === 'shut';
  const { reload } = project;
  useEffect(() => {
    if (!shut.current) return;
    reload();
    checkProviders().catch(() => {});
  }, [pathname, reload]);
  if (exempt(pathname) || gate === 'open') return children;
  return gate === 'shut' ? <Navigate to="/get-started" replace /> : null;
}
