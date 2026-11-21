// Untyped JSON (event payloads, rows) read field by field.

export type JsonObject = Record<string, unknown>;

/** The value as a record to read fields from: an object (arrays included) as it is, anything else as `{}`. */
export const asRecord = (v: unknown): JsonObject => (v && typeof v === 'object' ? (v as JsonObject) : {});
