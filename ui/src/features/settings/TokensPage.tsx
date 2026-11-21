// Settings → API tokens: tokens for the SDK and scripts. A new token is shown once; afterwards
// only its first characters.
import { useState } from 'react';
import { createToken, listTokens, type Minted, revokeToken, type TokenRow, type TokenScope } from '@/api/tokens';
import { Button } from '@/components/ui/button';
import { Field, Input } from '@/components/ui/input';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { Segmented } from '@/components/ui/segmented';
import { EmptyState, ErrorState, Notice } from '@/components/ui/state';
import { useAction } from '@/hooks/useAction';
import { useQuery } from '@/hooks/useQuery';
import SettingsPage from './SettingsPage';

/** Each scope and what a token with it may do. */
export const SCOPES: readonly { id: TokenScope; label: string; allows: string }[] = [
  { id: 'read', label: 'Read', allows: 'Read: sees runs, experiments and their records.' },
  { id: 'run', label: 'Run', allows: 'Run: also starts, forks, stops and records runs.' },
  { id: 'admin', label: 'Admin', allows: 'Admin: everything, including model keys and tokens.' },
];

export interface TokenForm { name: string; scope: TokenScope }

function TokenList({ tokens, onRevoke }: { tokens: TokenRow[]; onRevoke: (id: string) => void }) {
  if (tokens.length === 0) return <EmptyState title="No tokens yet." />;
  return (
    <ul className="divide-y divide-line">
      {tokens.map((t) => (
        <li key={t.id} className="flex items-center gap-3 px-4 py-2 text-sm">
          <span className="font-medium">{t.name}</span>
          <code className="font-mono text-xs text-muted">{t.prefix}…</code>
          <span className="text-xs text-muted">{t.scopes.join(', ')}</span>
          <span className="flex-1" />
          <Button variant="ghost" size="sm" onClick={() => onRevoke(t.id)}>Revoke</Button>
        </li>
      ))}
    </ul>
  );
}

export function TokensView({ tokens, fresh, form, onChange, onCreate, onRevoke, onDismiss }: {
  tokens: TokenRow[]; fresh: Minted | null; form: TokenForm;
  onChange: (f: TokenForm) => void; onCreate: () => void; onRevoke: (id: string) => void; onDismiss: () => void;
}) {
  const scope = SCOPES.find((s) => s.id === form.scope) ?? SCOPES[0];
  return (
    <>
      {fresh && (
        <Notice onDismiss={onDismiss}>
          <p className="text-sm">Copy your new token now: it is not shown again.</p>
          <code className="mt-1 block break-all font-mono text-xs">{fresh.token}</code>
        </Notice>
      )}
      <Panel>
        <PanelHeader title="New token" />
        <form className="flex flex-col gap-3 p-4" onSubmit={(e) => { e.preventDefault(); onCreate(); }}>
          <Field label="Name"><Input value={form.name} placeholder="ci" onChange={(e) => onChange({ ...form, name: e.target.value })} /></Field>
          <Segmented label="Scope" value={form.scope} options={SCOPES} onChange={(s) => onChange({ ...form, scope: s })} />
          <ul className="text-xs text-muted">{SCOPES.map((s) => <li key={s.id} className={s.id === scope.id ? 'text-fg' : ''}>{s.allows}</li>)}</ul>
          <Button type="submit" disabled={!form.name.trim()} className="self-start">Create token</Button>
        </form>
      </Panel>
      <Panel>
        <PanelHeader title="Tokens" />
        <TokenList tokens={tokens} onRevoke={onRevoke} />
      </Panel>
    </>
  );
}

export default function TokensPage() {
  const tokens = useQuery('tokens', listTokens);
  const [form, setForm] = useState<TokenForm>({ name: '', scope: 'read' });
  const [fresh, setFresh] = useState<Minted | null>(null);
  const create = useAction(async () => {
    setFresh(await createToken(form.name.trim(), [form.scope]));
    setForm({ name: '', scope: form.scope });
    tokens.reload();
  });
  const revoke = useAction(async (id: string) => { await revokeToken(id); tokens.reload(); });
  const error = tokens.error ?? create.error ?? revoke.error;
  return (
    <SettingsPage>
      {error && <ErrorState error={String(error)} />}
      <TokensView tokens={tokens.data ?? []} fresh={fresh} form={form} onChange={setForm}
        onCreate={() => { void create.run(); }} onRevoke={(id) => { void revoke.run(id); }} onDismiss={() => setFresh(null)} />
    </SettingsPage>
  );
}
