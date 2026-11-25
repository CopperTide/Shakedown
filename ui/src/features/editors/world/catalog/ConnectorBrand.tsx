import type { CatalogEntry } from './catalog';
export default function ConnectorBrand({ entry, name }: { entry?: CatalogEntry; name?: string }) {
  return <span className="grid size-10 shrink-0 place-items-center rounded-lg border border-line bg-white text-sm font-semibold text-slate-700" aria-hidden="true">
    {entry?.logo ? <img className="size-6 object-contain" src={`${import.meta.env.BASE_URL}connector-logos/${entry.logo}.svg`} alt="" /> : (entry?.name ?? name ?? 'API').slice(0, 2).toUpperCase()}
  </span>;
}
