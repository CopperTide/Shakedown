// Item ids become file and folder names: 1–64 lowercase letters, digits and dashes, not starting
// with a dash (the same rule the project folder checks).

export const ID_RULE = 'Use 1–64 lowercase letters, digits and dashes, starting with a letter or digit.';

export const isValidId = (s: string) => /^[a-z0-9][a-z0-9-]{0,63}$/.test(s);

/** A suggested id for `label`: lowercase, dashes for anything else. */
export function idFor(label: string): string {
  return label.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 64);
}
