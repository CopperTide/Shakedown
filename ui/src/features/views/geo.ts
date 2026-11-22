// Great-circle geometry for map layers: points along a route, a moving doc's place and heading at a time.
export type LonLat = [number, number];
const RAD = Math.PI / 180;

function vec([lon, lat]: LonLat): [number, number, number] {
  return [Math.cos(lat * RAD) * Math.cos(lon * RAD), Math.cos(lat * RAD) * Math.sin(lon * RAD), Math.sin(lat * RAD)];
}

/** The point a fraction `t` of the way along the great circle from `a` to `b`. */
export function along(a: LonLat, b: LonLat, t: number): LonLat {
  const p = vec(a), q = vec(b);
  const dot = Math.min(1, Math.max(-1, p[0] * q[0] + p[1] * q[1] + p[2] * q[2]));
  const w = Math.acos(dot);
  if (w < 1e-9) return a;
  const s = Math.sin(w), k1 = Math.sin((1 - t) * w) / s, k2 = Math.sin(t * w) / s;
  const x = k1 * p[0] + k2 * q[0], y = k1 * p[1] + k2 * q[1], z = k1 * p[2] + k2 * q[2];
  return [Math.atan2(y, x) / RAD, Math.atan2(z, Math.hypot(x, y)) / RAD];
}

/** `n` + 1 points of the great circle from `a` to `b`, longitudes unwrapped so a route over the antimeridian stays one line. */
export function route(a: LonLat, b: LonLat, n = 32): LonLat[] {
  const pts = Array.from({ length: n + 1 }, (_, i) => along(a, b, i / n));
  for (let i = 1; i < pts.length; i++) {
    let lon = pts[i][0];
    while (lon - pts[i - 1][0] > 180) lon -= 360;
    while (lon - pts[i - 1][0] < -180) lon += 360;
    pts[i] = [lon, pts[i][1]];
  }
  return pts;
}

/** The initial bearing from `a` to `b`, in degrees clockwise from north (0–360). */
export function bearing(a: LonLat, b: LonLat): number {
  const [l1, f1] = [a[0] * RAD, a[1] * RAD], [l2, f2] = [b[0] * RAD, b[1] * RAD];
  const y = Math.sin(l2 - l1) * Math.cos(f2);
  const x = Math.cos(f1) * Math.sin(f2) - Math.sin(f1) * Math.cos(f2) * Math.cos(l2 - l1);
  return ((Math.atan2(y, x) / RAD) + 360) % 360;
}

/** How far between `depart` and `arrive` time `now` is, clamped to 0–1; 0 when the times don't make an interval. */
export function fraction(now: number, depart: number | null, arrive: number | null): number {
  if (depart == null || arrive == null || !(arrive > depart)) return 0;
  return Math.min(1, Math.max(0, (now - depart) / (arrive - depart)));
}

export interface Placed {
  at: LonLat;
  /** Degrees clockwise from north. */
  heading: number;
  t: number;
}

/** Where a moving doc is at `now` on its great circle, and which way it heads. */
export function placeMoving(o: LonLat, d: LonLat, depart: number | null, arrive: number | null, now: number): Placed {
  const t = fraction(now, depart, arrive);
  const at = along(o, d, t);
  const ahead = t < 0.999 ? along(o, d, Math.min(1, t + 0.002)) : d;
  const heading = t < 0.999 ? bearing(at, ahead) : bearing(along(o, d, 0.998), d);
  return { at, heading, t };
}

/** The world time to draw at: the payload's `world_ms`, run on at `speed` world ms per real ms since it was fetched. */
export function worldNow(worldMs: number | null, speed: number, fetchedAt: number, nowPerf: number): number | null {
  if (worldMs == null) return null;
  if (!(speed > 0)) return worldMs;
  return worldMs + speed * Math.max(0, nowPerf - fetchedAt);
}
