// The account menu: shown only when the deployment's layer gives one in `/api/me` (a local shaked
// has none). It knows nothing of what its items mean: an item with `post` POSTs its `body` there,
// then the page reloads.
import { ChevronDown } from 'lucide-react';
import { useEffect, useState } from 'react';
import { api } from '@/api/core';
import { type AccountItem, fetchMe, type Me } from '@/api/me';
import { Button } from '@/components/ui/button';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu';

interface Deps { fetch: (url: string, init: RequestInit) => Promise<Response>; reload: () => void }

const browser: Deps = { fetch: (url, init) => window.fetch(url, init), reload: () => window.location.reload() };

/** Does what choosing `item` means: POSTs its body where it says, then reloads; nothing for the current one. */
export async function choose(item: AccountItem, deps: Deps = browser): Promise<void> {
  if (!item.post) return;
  await deps.fetch(api(item.post), {
    method: 'POST', credentials: 'include', headers: { 'content-type': 'application/json' }, body: JSON.stringify(item.body ?? {}),
  });
  deps.reload();
}

export function AccountMenu({ me }: { me: Me | null }) {
  const account = me?.account;
  if (!account) return null;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className="gap-1 text-sm">{account.label}<ChevronDown className="size-3.5 text-muted" /></Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="w-56">
        {account.items.map((item) => (
          <DropdownMenuItem key={item.label} onSelect={() => { void choose(item); }}>
            <span className="flex-1">{item.label}</span>{item.current && <span className="text-2xs text-muted">current</span>}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** `/api/me`, read once; null until it answers (or when it fails). */
export function useMe(): Me | null {
  const [me, setMe] = useState<Me | null>(null);
  useEffect(() => {
    let live = true;
    fetchMe().then((m) => { if (live) setMe(m); }, () => {});
    return () => { live = false; };
  }, []);
  return me;
}
