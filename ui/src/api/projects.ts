// Hosted project selection. Capability advertised by /api/me; local homes have one project.
import { get, path, post } from './client';

export interface ProjectChoice { id: string; name: string }
export interface ProjectChoices { projects: ProjectChoice[]; current: string | null }
export const fetchProjects = () => get<ProjectChoices>('/api/projects');
export const createProject = (name: string) => post<{ id: string }>('/api/projects', { name });
export const selectProject = (id: string) => post<{ id: string }>(path`/api/projects/${id}/select`);
