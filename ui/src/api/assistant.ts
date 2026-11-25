// The assistant: a question with the conversation so far and the page it was asked on.
import { post } from './client';

export interface ToolUse { name: string; summary: string }

export type Card =
  | ({ kind: 'diff'; tool: string; arguments: { id?: string; body?: unknown } & Record<string, unknown>; diff: string } & Settled)
  | ({ kind: 'confirm'; tool: string; arguments: Record<string, unknown>; access: 'spend' | 'delete' | 'edit' } & Settled);

/** How the person settled a card, kept with its message so a reopened drawer shows it settled. */
export interface Settled { outcome?: 'done' | 'dismissed' }

export interface ChatAnswer {
  reply: string;
  /** The conversation to send with the next question. */
  messages: unknown[];
  tools: ToolUse[];
  cards: Card[];
}

export const chat = (messages: unknown[], page: string) => post<ChatAnswer>('/api/assistant/chat', { messages, page });
