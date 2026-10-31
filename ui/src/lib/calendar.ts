// Date helpers for the planner.

export function sameDay(a: Date, b: Date): boolean {
  return a.toDateString() === b.toDateString();
}

export function startOfWeek(d: Date): Date {
  const s = new Date(d);
  s.setHours(0, 0, 0, 0);
  s.setDate(s.getDate() - s.getDay());
  return s;
}

export function addDays(d: Date, n: number): Date {
  const r = new Date(d);
  r.setDate(r.getDate() + n);
  return r;
}

/** ISO-8601 week number (Thunderbird's "CW"). */
export function isoWeek(d: Date): number {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const day = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - day);
  const yearStart = new Date(Date.UTC(t.getUTCFullYear(), 0, 1));
  return Math.ceil(((t.getTime() - yearStart.getTime()) / 86400000 + 1) / 7);
}

/** Whether an all-day event (stored as UTC midnights, end exclusive) covers a local day. */
export function allDayCovers(startIso: string, endIso: string, day: Date): boolean {
  const key = Date.UTC(day.getFullYear(), day.getMonth(), day.getDate());
  return new Date(startIso).getTime() <= key && key < new Date(endIso).getTime();
}

/** Assign side-by-side lanes to overlapping timed events of one day. */
export function lanes<T extends { start: string; end: string }>(events: T[]): { ev: T; lane: number; lanes: number }[] {
  const sorted = [...events].sort((a, b) => +new Date(a.start) - +new Date(b.start));
  const out: { ev: T; lane: number; lanes: number }[] = [];
  let group: { ev: T; lane: number; lanes: number }[] = [];
  let groupEnd = 0;
  const laneEnds: number[] = [];
  const flush = () => {
    const n = Math.max(1, ...group.map((g) => g.lane + 1));
    group.forEach((g) => (g.lanes = n));
    out.push(...group);
    group = [];
    laneEnds.length = 0;
  };
  for (const ev of sorted) {
    const s = +new Date(ev.start);
    const e = Math.max(+new Date(ev.end), s + 15 * 60000);
    if (group.length && s >= groupEnd) flush();
    let lane = laneEnds.findIndex((end) => end <= s);
    if (lane === -1) lane = laneEnds.length;
    laneEnds[lane] = e;
    group.push({ ev, lane, lanes: 1 });
    groupEnd = Math.max(groupEnd, e);
  }
  flush();
  return out;
}
