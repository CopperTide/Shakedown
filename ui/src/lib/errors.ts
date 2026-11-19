// What went wrong, as one line of text, whatever was thrown.

/** An Error's message, an object's `message`, else the value as text (never throws, even on nothing). */
export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (e && typeof e === 'object' && typeof (e as { message?: unknown }).message === 'string') return (e as { message: string }).message;
  return String(e);
}
