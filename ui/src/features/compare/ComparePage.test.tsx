import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { CompareSide } from '@/model/types';
import { Side } from './ComparePage';

const text = (html: string) => html.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ');

const side: CompareSide = { status: 'passed', seconds: 1, tokens: 1, cost_usd: null, cost_source: null };

describe('compare sides', () => {
  it('lists each agent with its model and provider; OpenAI cost is unknown', () => {
    const team = { id: 't', name: 't', agents: [
      { id: 'triage', name: 'Triage', kind: 'claude_code' as const, model: 'sonnet', prompt: '' },
      { id: 'resolver', name: 'Resolver', kind: 'open' as const, provider: 'openai', model: 'gpt-x', prompt: '' },
    ] };
    const html = text(renderToStaticMarkup(<Side id="abcdef0123" s={side} team={team} />));
    expect(html).toContain('Cost unknown');
    expect(html).toContain('Triage: Claude Code · Sonnet');
    expect(html).toContain('Resolver: OpenAI · gpt-x');
    const local = { ...side, cost_usd: 0, cost_source: 'local (no API charge)' };
    expect(text(renderToStaticMarkup(<Side id="b" s={local} />))).toContain('Cost no API charge');
  });
});
