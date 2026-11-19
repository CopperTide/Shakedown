// Paths and queries with their values encoded once, here: for arenad's API and the UI's own routes.

/** A path whose interpolated values are encoded as segments: path`/api/branches/${id}/rows`. */
export function path(strings: TemplateStringsArray, ...values: (string | number)[]): string {
  return strings.reduce((out, s, i) => out + s + (i < values.length ? encodeURIComponent(values[i]) : ''), '');
}

/** "?a=1&b=x%2Fy": a query, in the order given, each value encoded. */
export const qs = (params: Record<string, string | number>) =>
  `?${Object.entries(params).map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join('&')}`;
