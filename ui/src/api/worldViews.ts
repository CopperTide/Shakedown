// A world's declarative views on a branch: the parsed view files, and one screen evaluated at a point.
import type { ScreenData, WorldViews } from '@/features/views/types';
import { get, path, qs } from './client';

/** The branch's world views (`{world, views}`; `views` empty when its world has none). */
export const worldViews = (id: string) => get<WorldViews>(path`/api/branches/${id}/world-views`);

/** One screen of a view at `at`: `live`, `final` or a step's seq; null asks for core's default point. */
export const worldViewScreen = (id: string, view: string, screen: string, at: string | null) =>
  get<ScreenData>(path`/api/branches/${id}/world-views/${view}` + qs(at == null ? { screen } : { screen, at }));
