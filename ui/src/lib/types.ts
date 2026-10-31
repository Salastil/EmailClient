// Mirrors crates/mail-core/src/model.rs and settings.rs.

export type Provider = 'gmail' | 'outlook' | 'yahoo' | 'icloud' | 'fastmail' | 'cockli' | 'proton' | 'imap';
export type NotifyMode = 'all' | 'important_only' | 'muted';
export type FolderRole =
  | 'inbox' | 'starred' | 'sent' | 'drafts' | 'archive' | 'spam' | 'trash' | 'all' | 'label' | 'other';

export interface Account {
  id: string;
  name: string;
  email: string;
  provider: Provider;
  color: string;
  unread: number;
  notify: NotifyMode;
}

export interface Folder {
  id: string;
  account_id: string;
  name: string;
  role: FolderRole;
  unread: number;
  total: number;
  children: Folder[];
}

export interface Address {
  name: string | null;
  email: string;
}

export interface MessageSummary {
  id: string;
  account_id: string;
  folder_id: string;
  message_id: string;
  in_reply_to: string | null;
  references: string[];
  thread_hint: string | null;
  subject: string;
  from: Address;
  date: string;
  unread: boolean;
  flagged: boolean;
  important: boolean;
  has_attachments: boolean;
}

export interface ThreadNode {
  message: MessageSummary;
  reply_count: number;
  children: ThreadNode[];
}

export interface Attachment {
  name: string;
  mime: string;
  size: number;
}

export interface MessageView {
  message: {
    summary: MessageSummary;
    to: Address[];
    cc: Address[];
    body_text: string | null;
    attachments: Attachment[];
  };
  body: { html: string; blocked_remote: number } | null;
}

export type RuleField = 'sender' | 'domain' | 'subject';

export interface Settings {
  layout: { folders_width: number; list_width: number };
  tray: { close_to_tray: boolean; minimize_to_tray: boolean; start_minimized: boolean };
  privacy: { load_remote_images: boolean };
  notifications: {
    do_not_disturb: boolean;
    quiet_hours: [number, number] | null;
    vip: string[];
    rules: { field: RuleField; contains: string }[];
    use_provider_important: boolean;
    use_contacts: boolean;
    use_flagged: boolean;
    use_replied_threads: boolean;
    muted_folders: FolderRole[];
  };
  account_notify: Record<string, NotifyMode>;
}
