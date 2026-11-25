import { CATALOG, type CatalogEntry } from './catalog';
export interface CatalogFilter { query: string; category: string; industry: string; vendor: string; protocol: string }
export const EMPTY_FILTER: CatalogFilter = { query: '', category: '', industry: '', vendor: '', protocol: '' };
export function findConnectors(f: CatalogFilter): CatalogEntry[] {
  const words = f.query.toLowerCase().trim().split(/\s+/).filter(Boolean);
  return CATALOG.filter((c) => (!f.category || c.category === f.category) && (!f.industry || c.industries.includes(f.industry)) &&
    (!f.vendor || c.vendor === f.vendor) && (!f.protocol || c.protocols.includes(f.protocol)) &&
    words.every((w) => [c.name, c.vendor, c.category, ...c.industries, ...c.protocols, ...c.areas].join(' ').toLowerCase().includes(w)));
}
export const catalogEntry = (product?: string, label = '') => CATALOG.find((c) => c.id === product) ?? CATALOG.find((c) => label.toLowerCase().includes(c.name.toLowerCase()));
