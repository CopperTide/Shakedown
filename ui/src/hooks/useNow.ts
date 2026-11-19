import { useEffect, useState } from 'react';

/** The current time, every `ms` while `on`: a live run's clock and timeline move with it. */
export function useNow(on: boolean, ms = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!on) return;
    const id = window.setInterval(() => setNow(Date.now()), ms);
    return () => window.clearInterval(id);
  }, [on, ms]);
  return on ? now : Date.now();
}
