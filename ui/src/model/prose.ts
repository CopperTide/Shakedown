// The prose in what agents and models said: a message's content, a model's reply. Pure.
import { asRecord } from '@/lib/record';

/** The prose in a message's or a reply's content: a string, or the text of its `{type: "text"}` parts (null when it has none). */
export function proseOf(c: unknown): string | null {
  if (typeof c === 'string') return c;
  if (!Array.isArray(c)) return null;
  const texts = c.map((part) => asRecord(part)).filter((part) => part.type === 'text' && typeof part.text === 'string').map((part) => part.text as string);
  return texts.length ? texts.join('\n\n') : null;
}

/** What a model answered, as prose: OpenAI-style `choices[0].message.content` or Anthropic-style `content` parts. */
export function replyOf(response: unknown): string | null {
  const r = asRecord(response);
  const choice = Array.isArray(r.choices) ? asRecord(r.choices[0]) : null;
  return proseOf(choice ? asRecord(choice.message).content : r.content);
}
