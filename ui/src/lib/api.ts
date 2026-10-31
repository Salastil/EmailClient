import { invoke } from '@tauri-apps/api/core';
import type {
  Account, AccountConfig, Address, Discovered, Draft, Folder, MessageSummary, MessageView, NewAccount,
  NotifyMode, Settings, ThreadNode,
} from './types';

export const api = {
  listAccounts: () => invoke<Account[]>('list_accounts'),
  listFolders: (accountId: string) => invoke<Folder[]>('list_folders', { accountId }),
  listThreads: (folderId: string) => invoke<ThreadNode[]>('list_threads', { folderId }),
  search: (query: string, accountId: string | null) => invoke<MessageSummary[]>('search', { query, accountId }),
  getMessage: (id: string, allowRemote = false) => invoke<MessageView>('get_message', { id, allowRemote }),
  openAttachment: (id: string, index: number) => invoke<void>('open_attachment', { id, index }),
  saveAttachment: (id: string, index: number) => invoke<string | null>('save_attachment', { id, index }),
  setFlag: (id: string, flag: 'seen' | 'flagged', on: boolean) => invoke<void>('set_flag', { id, flag, on }),
  moveMessage: (id: string, folderId: string) => invoke<void>('move_message', { id, folderId }),
  archiveMessage: (id: string) => invoke<void>('archive_message', { id }),
  deleteMessage: (id: string) => invoke<void>('delete_message', { id }),
  syncNow: (accountId: string | null = null) => invoke<void>('sync_now', { accountId }),
  sendMessage: (draft: Draft) => invoke<void>('send_message', { draft }),
  completeAddress: (prefix: string) => invoke<Address[]>('complete_address', { prefix }),
  pickAttachments: () => invoke<string[]>('pick_attachments'),
  discoverAccount: (email: string) => invoke<Discovered>('discover_account', { email }),
  addAccount: (input: NewAccount) => invoke<Account>('add_account', { input }),
  cancelOAuth: () => invoke<void>('cancel_oauth'),
  reauthenticate: (accountId: string, password: string | null) => invoke<void>('reauthenticate', { accountId, password }),
  updateAccount: (accountId: string, update: { name: string; sender_name: string; color: string }) =>
    invoke<void>('update_account', { accountId, update }),
  removeAccount: (accountId: string) => invoke<void>('remove_account', { accountId }),
  accountDetails: (accountId: string) => invoke<AccountConfig>('account_details', { accountId }),
  getSettings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),
  setAccountNotify: (accountId: string, mode: NotifyMode) => invoke<void>('set_account_notify', { accountId, mode }),
  testNotification: (accountId: string) => invoke<boolean>('test_notification', { accountId }),
  quit: () => invoke<void>('quit'),
};
