import type { Address, FolderRole, Provider } from './types';

export function displayName(a: Address): string {
  return a.name?.trim() || a.email;
}

export function fullAddress(a: Address): string {
  return a.name ? `${a.name} <${a.email}>` : a.email;
}

/** Thunderbird-style short date: time today, otherwise M/D/YY with time. */
export function shortDate(iso: string, now = new Date()): string {
  const d = new Date(iso);
  const time = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  if (d.toDateString() === now.toDateString()) return time;
  const yy = String(d.getFullYear()).slice(-2);
  return `${d.getMonth() + 1}/${d.getDate()}/${yy}, ${time}`;
}

export function longDate(iso: string): string {
  return new Date(iso).toLocaleString([], { dateStyle: 'full', timeStyle: 'short' });
}

export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function initials(s: string): string {
  const parts = s.replace(/[<>@.]/g, ' ').trim().split(/\s+/);
  return ((parts[0]?.[0] ?? '?') + (parts[1]?.[0] ?? '')).toUpperCase();
}

/** Stable pleasant colour for an avatar derived from a string. */
export function hashColor(s: string): string {
  let h = 0;
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return `hsl(${h % 360} 45% 45%)`;
}

export const folderIcon: Record<FolderRole, string> = {
  inbox: 'inbox',
  starred: 'star',
  sent: 'send',
  drafts: 'draft',
  archive: 'archive',
  spam: 'report',
  trash: 'delete',
  all: 'stacks',
  important: 'label_important',
  label: 'label',
  other: 'folder',
};

export const providerIcon: Record<Provider, string> = {
  gmail: 'G',
  outlook: 'O',
  yahoo: 'Y!',
  icloud: '',
  fastmail: 'F',
  cockli: 'C',
  proton: 'P',
  imap: '@',
};
