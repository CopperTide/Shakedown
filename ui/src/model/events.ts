// Reading a run's record by kind: an event's payload typed for its kind, and the run's start.
import type { Event, EventKind, Payloads, RunStartPayload, TypedEvent } from './types';

/** True when `e` is of `kind`; its payload then has that kind's shape. */
export const isKind = <K extends EventKind>(e: Event, kind: K): e is Extract<TypedEvent, { kind: K }> => e.kind === kind;

/** `e`'s payload when it is of `kind`, else null. */
export function payloadOf<K extends EventKind>(e: Event | null | undefined, kind: K): Payloads[K] | null {
  // The kind says the shape (TypeScript can't follow a generic key through the union by itself).
  return e && e.kind === kind ? (e.payload as Payloads[K]) : null;
}

/** The payload of the last event of `kind` (the check a run was scored with), or null when there is none. */
export function lastPayload<K extends EventKind>(events: Event[], kind: K): Payloads[K] | null {
  for (let i = events.length - 1; i >= 0; i--) if (events[i].kind === kind) return payloadOf(events[i], kind);
  return null;
}

/** The run's `run_start` payload (absent in a branch whose record doesn't have it). */
export const runStartOf = (events: Event[]): RunStartPayload | undefined =>
  payloadOf(events.find((e) => e.kind === 'run_start'), 'run_start') ?? undefined;
