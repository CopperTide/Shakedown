import { useCallback, useEffect, useState } from 'react';
import { suggestedTeams } from '@/api';
import { errorMessage } from '@/lib/errors';
import type { Team } from '@/model/types';
import { loadTeam, saveTeam } from './storedTeam';

/** The team new runs use (kept in this browser), and a way to replace it (an import). */
export function useTeam() {
  const [team, setTeam] = useState<Team | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { loadTeam().then(setTeam).catch((e) => setError(errorMessage(e))); }, []);
  const replace = useCallback((t: Team) => { saveTeam(t); setTeam(t); }, []);
  return { team, error, replace };
}

/** The teams the benchmark suggests, the default first (empty until they load, or when they can't). */
export function useSuggestedTeams(): Team[] {
  const [teams, setTeams] = useState<Team[]>([]);
  useEffect(() => { suggestedTeams().then(setTeams).catch(() => { /* the picker is optional */ }); }, []);
  return teams;
}
