import { Suspense, lazy, useMemo } from 'react';
import { layerSummary, mapLayers } from './mapLayers';
import { PanelFrame } from './PanelFrame';
import type { At, MapData, MapDef } from './types';

const MapView = lazy(() => import('./MapView'));

/** A map panel: the layers' marks over the base map, which loads on first use. */
export default function MapPanel({ id, def, data, at, fetchedAt }: { id: string; def: MapDef; data: MapData; at: At | null; fetchedAt: number }) {
  const layers = useMemo(() => mapLayers(def, data), [def, data]);
  return (
    <PanelFrame id={id} title={def.title} meta={layerSummary(layers)} bodyClass="bg-[var(--map-land)]">
      <Suspense fallback={<div className="absolute inset-0 grid place-items-center text-xs text-faint">Loading the map…</div>}>
        <MapView def={def} layers={layers} at={at} fetchedAt={fetchedAt} />
      </Suspense>
    </PanelFrame>
  );
}
