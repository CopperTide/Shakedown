import { useCallback, useEffect, useState } from 'react';
import { defaultTeam, suggestedTeams } from '@/api/benchmark';
import { errorMessage } from '@/lib/errors';
import type { Team } from '@/model/types';
import { clearStoredTeam, loadTeam, saveTeam } from './storedTeam';
import { useQuery } from './useQuery';

/**
 * The team new runs use (kept in this browser, else the benchmark's default): `replace` it with
 * another (an import, saved), `update` it in place (an edit), `reset` it to the default.
 */
export function useStoredTeam() {
  const [team, setTeam] = useState<Team | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { loadTeam().then(setTeam).catch((e) => setError(errorMessage(e))); }, []);
  const replace = useCallback((t: Team) => { saveTeam(t); setTeam(t); }, []);
  const update = useCallback((fn: (t: Team) => Team) => setTeam((t) => (t ? fn(t) : t)), []);
  const reset = useCallback(async () => {
    clearStoredTeam();
    try { setTeam(await defaultTeam()); } catch (e) { setError(errorMessage(e)); }
  }, []);
  return { team, error, replace, update, reset };
}

/** The teams the benchmark suggests, the default first (empty until they load, or when they can't): read once per page. */
export function useSuggestedTeams(): Team[] {
  return useQuery('suggested-teams', suggestedTeams, { shared: true }).data ?? [];
}
