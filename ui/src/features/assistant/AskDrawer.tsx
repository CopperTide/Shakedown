// The Ask drawer: a chat with the assistant, on every page, opened from the top bar or ⌘J.
import { Sparkles, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { chat } from '@/api/assistant';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/input';
import { crumbsFor } from '@/features/shell/nav';
import { ASK_EVENT } from '@/features/shell/TopBar';
import { errorMessage } from '@/lib/errors';
import Message, { type Turn } from './Message';

function useOpen(onPrompt: (prompt: string) => void) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const show = (e: Event) => {
      setOpen(true);
      const prompt = (e as CustomEvent<{ prompt?: string } | undefined>).detail?.prompt;
      if (prompt) onPrompt(prompt);
    };
    const key = (e: KeyboardEvent) => { if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'j') { e.preventDefault(); setOpen((o) => !o); } };
    window.addEventListener(ASK_EVENT, show);
    window.addEventListener('keydown', key);
    return () => { window.removeEventListener(ASK_EVENT, show); window.removeEventListener('keydown', key); };
  }, [onPrompt]);
  return [open, setOpen] as const;
}

function useConversation(page: string) {
  const [turns, setTurns] = useState<Turn[]>([]);
  const [convo, setConvo] = useState<unknown[]>([]);
  const [busy, setBusy] = useState(false);
  const send = async (text: string) => {
    setTurns((t) => [...t, { role: 'user', text }]);
    setBusy(true);
    try {
      const a = await chat([...convo, { role: 'user', content: text }], page);
      setConvo(a.messages);
      setTurns((t) => [...t, { role: 'assistant', text: a.reply, tools: a.tools, cards: a.cards }]);
    } catch (e) {
      setTurns((t) => [...t, { role: 'assistant', text: errorMessage(e) }]);
    } finally {
      setBusy(false);
    }
  };
  /** Keeps how the person settled a card with its message, so it never reopens. */
  const settle = (turn: number, card: number, outcome: 'done' | 'dismissed') =>
    setTurns((ts) => ts.map((t, i) => (i !== turn ? t : { ...t, cards: t.cards?.map((c, j) => (j === card ? { ...c, outcome } : c)) })));
  return { turns, busy, send, settle };
}

export default function AskDrawer() {
  const [text, setText] = useState('');
  const [open, setOpen] = useOpen(setText);
  const page = crumbsFor(useLocation().pathname).at(-1) ?? 'Home';
  const { turns, busy, send, settle } = useConversation(page);
  if (!open) return null;
  const go = () => { if (text.trim()) { void send(text.trim()); setText(''); } };
  return (
    <aside aria-label="Assistant" className="fixed inset-y-0 right-0 z-50 flex w-[min(440px,100vw)] flex-col border-l border-line bg-surface shadow-pop">
      <header className="flex items-center gap-3 border-b border-line px-4 py-3">
        <span className="grid size-7 place-items-center rounded-md bg-accent-soft text-copper"><Sparkles className="size-4" /></span>
        <div className="flex-1"><div className="text-sm font-semibold">Assistant</div><div className="text-2xs text-faint">Looking at {page}</div></div>
        <Button variant="ghost" size="icon" aria-label="Close" onClick={() => setOpen(false)}><X /></Button>
      </header>
      <div className="flex flex-1 flex-col gap-4 overflow-auto px-4 py-4">
        {turns.length === 0 && <p className="text-sm text-muted">Ask about this page, or tell it what to change.</p>}
        {turns.map((t, i) => <Message key={i} turn={t} onSettle={(card, outcome) => settle(i, card, outcome)} />)}
        {busy && <p className="text-xs text-faint">Thinking…</p>}
      </div>
      <footer className="grid gap-2 border-t border-line p-3">
        <Textarea aria-label="Ask the assistant" rows={2} value={text} onChange={(e) => setText(e.target.value)} placeholder="Ask, or tell it what to change…"
          onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); go(); } }} />
        <div className="flex items-center gap-2"><p className="flex-1 text-2xs text-faint">Reads run right away. File edits show a diff to apply. Anything that starts runs or spends money asks first.</p>
          <Button variant="primary" size="sm" disabled={busy} onClick={go}>Send</Button></div>
      </footer>
    </aside>
  );
}
