// The world's time zone for everything on a world view: set once from the screen's `at`, read by each panel or
// clock that shows a time, so every time on screen reads in the same zone.
import { createContext, useContext } from 'react';
import { UTC, type Zone } from './format';

export const ZoneContext = createContext<Zone>(UTC);

/** The zone the world view on screen shows its times in. */
export const useZone = () => useContext(ZoneContext);
