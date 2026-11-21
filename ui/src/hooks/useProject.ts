// The project folder's status, read once per page: open with its name and counts, or not open
// (shaked answers 409) with why.
import { projectStatus } from '@/api/project';
import { ApiError } from '@/api/client';
import type { ProjectStatus } from '@/model/project';
import { useQuery } from './useQuery';

export type ProjectState =
  | { open: true; status: ProjectStatus }
  | { open: false; message: string };

/** `/api/project`, with "no project" (409) as a state rather than an error. */
export async function loadProject(): Promise<ProjectState> {
  try {
    return { open: true, status: await projectStatus() };
  } catch (e) {
    if (e instanceof ApiError && e.status === 409) return { open: false, message: e.message };
    throw e;
  }
}

export function useProject() {
  return useQuery('project-status', loadProject, { shared: true });
}
