// What clicking a card does: the operation the card names, called through its guarded route, and
// what the card is called.
import type { Card } from '@/api/assistant';
import { post, path } from '@/api/client';
import { createProject, deleteItem, importProject, putItem, renameGroup, restoreDeleted, updateSettings } from '@/api/project';
import type { ItemKind, ProjectSettings } from '@/model/project';

const KINDS: ItemKind[] = ['worlds', 'scenarios', 'setups', 'experiments'];

/** `scenarios_put` → `['scenarios', 'put']`. */
export function split(tool: string): [string, string] {
  const at = tool.indexOf('_');
  return [tool.slice(0, at), tool.slice(at + 1)];
}

/** The kind of item a tool's area names, if it names one. */
export const asKind = (area: string) => (KINDS.includes(area as ItemKind) ? (area as ItemKind) : null);

type Args = Record<string, unknown>;
const text = (v: unknown) => String(v ?? '');

/** The operations a card names by tool, beside an item's put and delete; each sent confirmed. */
const BY_TOOL: Record<string, (args: Args) => Promise<unknown>> = {
  experiments_deploy: (a) => post(path`/api/project/experiments/${text(a.id)}/deploy`, {}),
  worlds_rename_group: (a) => renameGroup(text(a.from), text(a.to).trim() ? text(a.to) : null),
  project_init: (a) => createProject(text(a.name), Boolean(a.git)),
  project_update: (a) => updateSettings(a.body as ProjectSettings, typeof a.revision === 'number' ? a.revision : undefined),
  project_import: () => importProject(true),
  project_restore: (a) => restoreDeleted(a.trash == null ? undefined : text(a.trash)),
};

/** Runs `card`'s operation; throws its error. A delete card deletes the item alone. */
export async function applyCard(card: Card): Promise<unknown> {
  const [area, verb] = split(card.tool);
  const kind = asKind(area);
  const args = card.arguments as Args;
  if (card.kind === 'diff' && kind && verb === 'put') return putItem(kind, text(args.id), args.body, true, typeof args.revision === 'number' ? args.revision : undefined);
  if (kind && verb === 'delete') return deleteItem(kind, text(args.id));
  const run = BY_TOOL[card.tool];
  if (run) return run(args);
  throw new Error(`Shakedown can't apply ${card.tool} from here yet.`);
}

const TITLES: Record<string, string> = {
  worlds_rename_group: 'Rename world group',
  project_update: 'Change project settings',
  project_import: 'Move earlier work into the project',
  project_restore: 'Undo the last delete',
  project_init: 'Create the project',
  experiments_deploy: 'Deploy experiment',
};

/** A card's title: "Edit world mill", "Delete world mill", "Rename world group". */
export function cardTitle(card: Card): string {
  const [area, verb] = split(card.tool);
  const one = area.replace(/s$/, '');
  const id = text(card.arguments.id);
  if (verb === 'put') return `Edit ${one} ${id}`;
  if (verb === 'delete') return `Delete ${one} ${id}`;
  return TITLES[card.tool] ?? `${verb.replace(/_/g, ' ')} ${one}`;
}
