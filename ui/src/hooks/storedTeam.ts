// The team new runs use, kept in this browser (`arena.team.v1`), else the benchmark's default.
import { defaultTeam, suggestedTeams } from '@/api/benchmark';
import { isUntouched } from '@/model/team';
import type { Team } from '@/model/types';

export const TEAM_KEY = 'arena.team.v1';

function valid(t: unknown): t is Team {
  const x = t as Team | null;
  return !!x && typeof x.id === 'string' && typeof x.name === 'string' && Array.isArray(x.agents) && x.agents.length > 0;
}

export function readStoredTeam(): Team | null {
  try {
    const raw = localStorage.getItem(TEAM_KEY);
    if (!raw) return null;
    const t = JSON.parse(raw);
    return valid(t) ? t : null;
  } catch {
    return null;
  }
}

export function saveTeam(team: Team): void {
  try {
    localStorage.setItem(TEAM_KEY, JSON.stringify(team));
  } catch {
    /* storage unavailable: the team lives for this page only */
  }
}

export function clearStoredTeam(): void {
  try {
    localStorage.removeItem(TEAM_KEY);
  } catch {
    /* ignore */
  }
}

/**
 * The stored team, else the server's default. A stored team that is an untouched suggestion other
 * than the default (an old default the browser kept) gives way to the default.
 */
export async function loadTeam(): Promise<Team> {
  const stored = readStoredTeam();
  if (!stored) return defaultTeam();
  const suggested = await suggestedTeams().catch(() => [] as Team[]);
  const first = suggested[0];
  if (first && stored.id !== first.id && isUntouched(stored, suggested.slice(1))) {
    clearStoredTeam();
    return first;
  }
  return stored;
}
