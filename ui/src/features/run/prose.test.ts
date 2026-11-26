import { describe, expect, it } from 'vitest';
import { proseOf, replyOf } from './Inspector';

describe('prose of messages and replies', () => {
  it('takes a string, or the text parts of a content list', () => {
    expect(proseOf('plain')).toBe('plain');
    expect(proseOf([{ type: 'text', text: 'a' }, { type: 'image_url' }, { type: 'text', text: 'b' }])).toBe('a\n\nb');
    expect(proseOf([{ type: 'tool_use' }])).toBeNull();
    expect(proseOf(undefined)).toBeNull();
  });
  it('reads OpenAI-style and Anthropic-style replies', () => {
    expect(replyOf({ choices: [{ message: { content: '**done**' } }] })).toBe('**done**');
    expect(replyOf({ content: [{ type: 'text', text: 'ok' }] })).toBe('ok');
    expect(replyOf({ choices: [{ message: { content: null, tool_calls: [] } }] })).toBeNull();
  });
});
