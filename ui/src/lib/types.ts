// Mirrors crates/mail-core/src/model.rs and settings.rs.

export type Provider = 'gmail' | 'outlook' | 'yahoo' | 'icloud' | 'fastmail' | 'cockli' | 'proton' | 'imap';
export type NotifyMode = 'all' | 'important_only' | 'muted';
export type FolderRole =
  | 'inbox' | 'starred' | 'sent' | 'drafts' | 'archive' | 'spam' | 'trash' | 'all' | 'label' | 'other';

export type AccountStatus =
  | { state: 'idle' }
  | { state: 'syncing' }
  | { state: 'offline'; message: string }
  | { state: 'error'; message: string }
  | { state: 'needs_auth'; message: string };

export interface Account {
  id: string;
  name: string;
  email: string;
  provider: Provider;
  color: string;
  unread: number;
  notify: NotifyMode;
  status: AccountStatus;
}

export type Security = 'tls' | 'starttls' | 'none';
export type OAuthProvider = 'google' | 'microsoft' | 'yahoo' | 'aol' | 'fastmail';

export interface ServerConfig {
  host: string;
  port: number;
  security: Security;
}

export interface Discovered {
  provider: Provider;
  imap: ServerConfig;
  smtp: ServerConfig;
  username_is_email: boolean;
  oauth: OAuthProvider | null;
  proton: boolean;
  source: string;
}

export interface AccountConfig {
  id: string;
  name: string;
  email: string;
  sender_name: string;
  provider: Provider;
  color: string;
  username: string;
  auth: { kind: 'password' } | { kind: 'o_auth2'; provider: OAuthProvider } | { kind: 'proton_bridge' };
  imap: ServerConfig;
  smtp: ServerConfig;
}

export interface NewAccount {
  email: string;
  name?: string;
  sender_name: string;
  provider: Provider;
  username: string;
  password?: string;
  imap: ServerConfig;
  smtp: ServerConfig;
  oauth?: OAuthProvider | null;
  proton?: boolean;
  totp?: string;
  mailbox_password?: string;
}

export interface Draft {
  account_id: string;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  text: string;
  reply_to_id?: string | null;
  forward_id?: string | null;
  attachments: string[];
}

export interface Folder {
  id: string;
  account_id: string;
  path: string;
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
  index: number;
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
  oauth_clients: Record<string, { client_id: string; client_secret: string | null }>;
  account_order: string[];
}

export interface ComposeInit {
  account_id: string | null;
  to?: string;
  cc?: string;
  subject?: string;
  text?: string;
  reply_to_id?: string | null;
  forward_id?: string | null;
  forwarded_attachments?: string[];
}

export type MessageAction = 'reply' | 'reply_all' | 'forward' | 'archive' | 'delete' | 'toggle_read' | 'toggle_star';
