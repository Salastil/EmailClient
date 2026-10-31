// Calendar model and demo data until CalDAV sync (crates/pim) lands.

export interface CalendarSource {
  id: string;
  name: string;
  color: string;
  readonly: boolean;
  enabled: boolean;
  kind: 'local' | 'google' | 'caldav' | 'microsoft';
}

export interface CalEvent {
  id: string;
  calendar: string;
  title: string;
  start: Date;
  end: Date;
  allDay: boolean;
  reminder: boolean;
}

export interface Task {
  id: string;
  title: string;
  done: boolean;
  due?: Date;
}

export const calendars: CalendarSource[] = [
  { id: 'home', name: 'Home', color: '#9aa1a9', readonly: false, enabled: true, kind: 'local' },
  { id: 'google', name: 'you@gmail.com', color: '#7bd148', readonly: false, enabled: true, kind: 'google' },
  { id: 'holidays', name: 'Holidays in United States', color: '#2ecc9a', readonly: true, enabled: true, kind: 'google' },
  { id: 'nextcloud', name: 'Nextcloud', color: '#4dabf7', readonly: false, enabled: true, kind: 'caldav' },
];

function at(base: Date, dayOffset: number, h: number, m = 0): Date {
  const d = new Date(base);
  d.setDate(d.getDate() + dayOffset);
  d.setHours(h, m, 0, 0);
  return d;
}

/** Demo events placed relative to the current week so the view is never empty. */
export function demoEvents(now = new Date()): CalEvent[] {
  const sunday = new Date(now);
  sunday.setDate(now.getDate() - now.getDay());
  return [
    { id: 'e1', calendar: 'google', title: 'Lawn cutting', start: at(sunday, 1, 0), end: at(sunday, 2, 0), allDay: true, reminder: true },
    { id: 'e2', calendar: 'google', title: 'Nurse visit', start: at(sunday, 3, 12), end: at(sunday, 3, 13), allDay: false, reminder: true },
    { id: 'e3', calendar: 'nextcloud', title: 'State Farm John', start: at(sunday, 5, 14, 30), end: at(sunday, 5, 15, 15), allDay: false, reminder: false },
    { id: 'e4', calendar: 'holidays', title: 'Halloween', start: at(sunday, 6, 0), end: at(sunday, 7, 0), allDay: true, reminder: false },
    { id: 'e5', calendar: 'home', title: 'Groceries', start: at(sunday, 2, 9), end: at(sunday, 2, 10), allDay: false, reminder: false },
  ];
}

export const demoTasks: Task[] = [
  { id: 't1', title: 'Pay rent', done: false },
  { id: 't2', title: 'Call John about renewal', done: false },
  { id: 't3', title: 'Renew domain', done: true },
];

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
