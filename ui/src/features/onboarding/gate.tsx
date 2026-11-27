// The onboarding gate: until setup is done (a project and a model, as shaked reports them), every
// page but Get started sends you there; setup is done entirely on that page. The state comes from the server on every navigation while the gate is shut, never from
// the browser.
import { type ReactNode, useEffect, useRef } from 'react';
import { Navigate, useLocation } from 'react-router-dom';
import { type ProjectState, useProject } from '@/hooks/useProject';
import { checkProviders, useCheckedProviders } from '@/hooks/useProviders';
import type { ProviderStatus } from '@/model/types';
import { coreDone, stageOf, stepStates } from './steps';

/** Pages open before setup is done: only Get started itself. */
export const EXEMPT: RegExp[] = [/^\/get-started(\/|$)/];

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
  return coreDone(stepStates({ project: stage, providersOk: providers.some((p) => p.available) })) ? 'open' : 'shut';
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
