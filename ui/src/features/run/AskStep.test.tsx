// @vitest-environment jsdom
// "Ask about this step": the step's questions with their answers (evidence quoted from the record,
// speculation labelled), who asked and what it cost; a new question is sent with the model picked.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import AskStep from './AskStep';

const answered = {
  key: 'ask:b:4:x', kind: 'ask', status: 'done', model: 'openrouter:m', issues: ['dropped evidence from m3.2: the quote is not in it word for word'],
  error: null, cost_usd: 0.0123, cost_source: 'provider', tokens: 1000, seconds: 2, digest_bytes: 900,
  created_at: '2026-11-25T10:00:00Z', finished_at: '2026-11-25T10:00:02Z',
  review: {
    question: 'Why approval?', step: 4, agent: 'ops', asked_by: 'person', mine: true,
    answer: 'The system prompt sets a $500 limit.',
    evidence: [{ source: 'm3.0', step: 3, field: 'request.messages[0] (system)', quote: 'Refunds over $500 need approval.', why: 'the rule' }],
    speculation: [{ text: 'Likely anchored on the $500 line.', based_on: ['m3.0'] }],
  },
};

function mockFetch() {
  const calls: { url: string; body?: string }[] = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    calls.push({ url: u, body: init?.body as string | undefined });
    if (u.endsWith('/api/review/settings')) return new Response(JSON.stringify({ enabled: false, model: 'openrouter:m', ask: true }));
    if (init?.method === 'POST') return new Response(JSON.stringify({ ...answered, key: 'ask:b:4:y', status: 'running', review: { question: 'And then?', step: 4, agent: 'ops', asked_by: 'person', mine: true } }));
    if (u.includes('/questions')) return new Response(JSON.stringify({ questions: [answered] }));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
  return calls;
}

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('ask about this step', () => {
  it('shows_evidence_apart_from_speculation_with_who_asked_and_the_cost', async () => {
    mockFetch();
    const onStep = vi.fn();
    render(<AskStep branchId="b" seq={4} onStep={onStep} />);
    await waitFor(() => expect(screen.getByText('Why approval?')).toBeTruthy());
    expect(screen.getByText('Evidence')).toBeTruthy();
    expect(screen.getByText('Refunds over $500 need approval.')).toBeTruthy();
    expect(screen.getByText('Speculation')).toBeTruthy();
    expect(screen.getByText('1 citation dropped')).toBeTruthy();
    expect(screen.getByText(/^You · .* · openrouter:m · \$0\.012.* · 1,000 tokens$/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'step 3' }));
    expect(onStep).toHaveBeenCalledWith(3);
  });

  it('asks_with_the_model_picked_and_lists_the_question_while_it_runs', async () => {
    const calls = mockFetch();
    render(<AskStep branchId="b" seq={4} onStep={() => {}} />);
    const model = await screen.findByLabelText('Explainer model');
    expect((model as HTMLInputElement).value).toBe('openrouter:m');
    fireEvent.change(model, { target: { value: 'anthropic:claude-x' } });
    fireEvent.change(screen.getByLabelText('Your question'), { target: { value: 'And then?' } });
    fireEvent.click(screen.getByRole('button', { name: 'Ask' }));
    await waitFor(() => expect(screen.getByText('Looking into it…')).toBeTruthy());
    const post = calls.find((c) => c.body);
    expect(post?.url).toContain('/api/branches/b/questions');
    expect(JSON.parse(post!.body!)).toEqual({ seq: 4, question: 'And then?', model: 'anthropic:claude-x' });
  });
});
