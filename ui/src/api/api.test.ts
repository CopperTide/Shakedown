import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Event } from '@/model/types';
import { stream } from './branches';

class FakeEventSource {
  static last: FakeEventSource;
  listeners = new Map<string, (e: MessageEvent) => void>();
  closed = false;
  constructor(public url: string) {
    FakeEventSource.last = this;
  }
  addEventListener(name: string, fn: (e: MessageEvent) => void) {
    this.listeners.set(name, fn);
  }
  close() {
    this.closed = true;
  }
  emit(name: string, data: unknown) {
    this.listeners.get(name)?.({ data: JSON.stringify(data) } as MessageEvent);
  }
}

afterEach(() => vi.unstubAllGlobals());

describe('api', () => {
  it('starts a lean stream after the events already loaded by the page', () => {
    vi.stubGlobal('EventSource', FakeEventSource);
    stream('b1', () => {}, () => {}, { after: 800, lean: true });
    expect(FakeEventSource.last.url).toBe('/api/branches/b1/stream?after=800&lean=1');
  });
  it('stream_parses_events', () => {
    vi.stubGlobal('EventSource', FakeEventSource);
    const events: Event[] = [];
    const statuses: { status: string; error: string | null }[] = [];
    const stop = stream('b1', (e) => events.push(e), (s) => statuses.push(s));
    const es = FakeEventSource.last;
    expect(es.url).toBe('/api/branches/b1/stream');

    const ev = {
      branch_id: 'b1',
      seq: 3,
      at: '2026-10-30T00:00:00Z',
      agent: 'a1',
      kind: 'tool_call',
      payload: { name: 'find_user' },
    } as Event;
    es.emit('event', ev);
    es.emit('status', { status: 'passed', error: null });
    es.listeners.get('event')?.({ data: 'not json' } as MessageEvent); // ignored, no throw

    expect(events).toEqual([ev]);
    expect(statuses).toEqual([{ status: 'passed', error: null }]);
    stop();
    expect(es.closed).toBe(true);
  });
});
