import { invoke } from '@tauri-apps/api/core';
import type { Account, Folder, MessageView, NotifyMode, Settings, ThreadNode } from './types';

export const api = {
  listAccounts: () => invoke<Account[]>('list_accounts'),
  listFolders: (accountId: string) => invoke<Folder[]>('list_folders', { accountId }),
  listThreads: (folderId: string) => invoke<ThreadNode[]>('list_threads', { folderId }),
  getMessage: (id: string, allowRemote = false) => invoke<MessageView>('get_message', { id, allowRemote }),
  getSettings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),
  setAccountNotify: (accountId: string, mode: NotifyMode) =>
    invoke<void>('set_account_notify', { accountId, mode }),
  simulateIncoming: (accountId: string) => invoke<boolean>('simulate_incoming', { accountId }),
  quit: () => invoke<void>('quit'),
};
