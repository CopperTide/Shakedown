// The project folder's status, read once per page: open with its name and counts, or not open
// (shaked answers 409) with why, and whether a project made here is a folder on disk.
import { projectStatus } from '@/api/project';
import { ApiError } from '@/api/client';
import type { ProjectStatus } from '@/model/project';
import { useQuery } from './useQuery';

export type ProjectState =
  | { open: true; status: ProjectStatus }
  | { open: false; message: string; folder: boolean };

/** `/api/project`, with "no project" (409) as a state rather than an error. */
export async function loadProject(): Promise<ProjectState> {
  try {
    return { open: true, status: await projectStatus() };
  } catch (e) {
    if (e instanceof ApiError && e.status === 409) return { open: false, message: e.message, folder: (e.body as { folder?: unknown } | undefined)?.folder === true };
    throw e;
  }
}

export function useProject() {
  return useQuery('project-status', loadProject, { shared: true });
}
