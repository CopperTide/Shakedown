// What clicking a card does: the operation the card names, called through its guarded route.
import type { Card } from '@/api/assistant';
import { post, path } from '@/api/client';
import { createProject, deleteItem, putItem } from '@/api/project';
import type { ItemKind } from '@/model/project';

const KINDS: ItemKind[] = ['worlds', 'scenarios', 'setups', 'experiments'];

/** `scenarios_put` → `['scenarios', 'put']`. */
export function split(tool: string): [string, string] {
  const at = tool.indexOf('_');
  return [tool.slice(0, at), tool.slice(at + 1)];
}

const asKind = (area: string) => (KINDS.includes(area as ItemKind) ? (area as ItemKind) : null);

/** Runs `card`'s operation; throws its error. */
export async function applyCard(card: Card): Promise<unknown> {
  const [area, verb] = split(card.tool);
  const kind = asKind(area);
  if (card.kind === 'diff' && kind) return putItem(kind, card.arguments.id, card.arguments.body, true);
  const args = card.arguments as Record<string, unknown>;
  const id = String(args.id ?? '');
  if (kind === 'experiments' && verb === 'deploy') return post(path`/api/project/experiments/${id}/deploy`, {});
  if (kind && verb === 'delete') return deleteItem(kind, id);
  if (card.tool === 'project_init') return createProject(String(args.name ?? ''), Boolean(args.git));
  throw new Error(`Shakedown can't apply ${card.tool} from here yet.`);
}
