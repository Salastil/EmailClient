<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import AccountRail from './components/AccountRail.svelte';
  import AddAccount from './components/AddAccount.svelte';
  import CalendarView from './components/CalendarView.svelte';
  import Compose from './components/Compose.svelte';
  import Dialog from './components/Dialog.svelte';
  import FolderPane from './components/FolderPane.svelte';
  import Icon from './components/Icon.svelte';
  import MessageList from './components/MessageList.svelte';
  import Reader from './components/Reader.svelte';
  import SettingsPane from './components/SettingsPane.svelte';
  import Splitter from './components/Splitter.svelte';
  import { api } from './lib/api';
  import { displayName } from './lib/format';
  import type { Account, Folder, MessageSummary, MessageView, NotifyMode, Settings, ThreadNode } from './lib/types';

  let accounts = $state<Account[]>([]);
  let accountId = $state<string | null>(null);
  let folders = $state<Folder[]>([]);
  let folder = $state<Folder | null>(null);
  let threads = $state<ThreadNode[]>([]);
  let messageId = $state<string | null>(null);
  let message = $state<MessageView | null>(null);
  let settings = $state<Settings | null>(null);
  let view = $state<'mail' | 'calendar'>('mail');
  let settingsOpen = $state(false);
  let dialog = $state<null | 'account' | 'contact' | 'event'>(null);
  let compose = $state<null | { to: string; subject: string }>(null);
  let error = $state<string | null>(null);
  let search: HTMLInputElement;

  let foldersWidth = $state(240);
  let listWidth = $state(380);

  const account = $derived(accounts.find((a) => a.id === accountId) ?? null);

  async function guard<T>(p: Promise<T>): Promise<T | undefined> {
    try {
      return await p;
    } catch (e) {
      error = String(e);
    }
  }

  async function selectAccount(id: string) {
    view = 'mail';
    if (id === accountId && folders.length) return;
    accountId = id;
    folders = (await guard(api.listFolders(id))) ?? [];
    await selectFolder(folders.find((f) => f.role === 'inbox') ?? folders[0]);
  }

  async function selectFolder(f: Folder | undefined) {
    folder = f ?? null;
    messageId = null;
    message = null;
    threads = f ? ((await guard(api.listThreads(f.id))) ?? []) : [];
  }

  async function openMessage(m: MessageSummary, allowRemote = false) {
    messageId = m.id;
    m.unread = false;
    message = (await guard(api.getMessage(m.id, allowRemote))) ?? null;
  }

  function star(m: MessageSummary) {
    m.flagged = !m.flagged;
  }

  async function saveSettings(next: Settings) {
    settings = next;
    await guard(api.saveSettings(next));
  }

  function persistLayout() {
    if (!settings) return;
    saveSettings({ ...settings, layout: { folders_width: foldersWidth, list_width: listWidth } });
  }

  async function setNotify(id: string, mode: NotifyMode) {
    await guard(api.setAccountNotify(id, mode));
    const a = accounts.find((a) => a.id === id);
    if (a) a.notify = mode;
    if (settings) settings.account_notify[id] = mode;
  }

  function reply(kind: 'reply' | 'reply_all' | 'forward') {
    const s = message?.message.summary;
    if (!s) return;
    const prefix = kind === 'forward' ? 'Fwd: ' : 'Re: ';
    const subject = s.subject.replace(/^(re|fwd?):\s*/i, '');
    compose = { to: kind === 'forward' ? '' : s.from.email, subject: prefix + subject };
  }

  onMount(() => {
    (async () => {
      settings = (await guard(api.getSettings())) ?? null;
      if (settings) {
        foldersWidth = settings.layout.folders_width;
        listWidth = settings.layout.list_width;
      }
      accounts = (await guard(api.listAccounts())) ?? [];
      if (accounts.length) await selectAccount(accounts[0].id);
    })();
    const unlisten = listen('compose', () => (compose = { to: '', subject: '' }));
    return () => {
      unlisten.then((f) => f());
    };
  });

  function keys(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      search.focus();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
      e.preventDefault();
      compose = { to: '', subject: '' };
    }
  }
</script>

<svelte:window onkeydown={keys} />

<div class="app">
  <header class="topbar">
    <span class="brand"><Icon name="forum" size={20} fill /> Moho Mail</span>
    <label class="search">
      <Icon name="search" size={18} />
      <input bind:this={search} placeholder="Search mail, contacts and events" />
      <kbd>Ctrl K</kbd>
    </label>
    <span class="spacer"></span>
  </header>

  <div class="columns">
    <AccountRail
      {accounts}
      selected={accountId}
      {view}
      onselect={selectAccount}
      oncalendar={() => (view = view === 'calendar' ? 'mail' : 'calendar')}
      onsettings={() => (settingsOpen = true)}
      onadd={(what) => (dialog = what)}
      onnotify={setNotify}
    />

    {#if view === 'mail'}
      <FolderPane
        {account}
        {folders}
        selected={folder?.id ?? null}
        width={foldersWidth}
        onselect={selectFolder}
        oncompose={() => (compose = { to: '', subject: '' })}
      />
      <Splitter bind:width={foldersWidth} min={180} max={420} onrelease={persistLayout} />
      <MessageList
        {folder}
        {threads}
        selected={messageId}
        width={listWidth}
        onselect={(m) => openMessage(m)}
        onstar={star}
      />
      <Splitter bind:width={listWidth} min={280} max={720} onrelease={persistLayout} />
      <Reader
        view={message}
        onloadremote={() => message && openMessage(message.message.summary, true)}
        onreply={reply}
      />
    {:else}
      <CalendarView onnewevent={() => (dialog = 'event')} />
    {/if}
  </div>

  {#if error}
    <div class="snackbar" role="alert">
      {error}
      <button class="icon-btn" aria-label="Dismiss" onclick={() => (error = null)}><Icon name="close" size={18} /></button>
    </div>
  {/if}
</div>

{#if settingsOpen && settings}
  <SettingsPane
    {settings}
    {accounts}
    onchange={saveSettings}
    onnotify={setNotify}
    ontest={(id) => api.simulateIncoming(id)}
    onclose={() => (settingsOpen = false)}
  />
{/if}

{#if dialog === 'account'}
  <AddAccount onclose={() => (dialog = null)} />
{:else if dialog === 'contact'}
  <Dialog title="New contact" onclose={() => (dialog = null)}>
    <p class="todo">Contacts sync with CardDAV (Google, Nextcloud, Fastmail) in milestone 5.</p>
  </Dialog>
{:else if dialog === 'event'}
  <Dialog title="New event" onclose={() => (dialog = null)}>
    <p class="todo">Event editing syncs with CalDAV (Google, Nextcloud and others) in milestone 5.</p>
  </Dialog>
{/if}

{#if compose}
  <Compose from={account} to={compose.to} subject={compose.subject} onclose={() => (compose = null)} />
{/if}

<style>
  .app { height: 100%; display: flex; flex-direction: column; }
  .topbar {
    height: 40px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 12px;
    background: var(--md-sys-color-surface-container-lowest);
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }
  .brand { display: flex; align-items: center; gap: 8px; font-weight: 700; color: var(--md-sys-color-primary); width: 220px; }
  .spacer { width: 220px; }
  .search {
    flex: 0 1 420px;
    margin: 0 auto;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 6px 0 10px;
    border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
  }
  .search input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font-size: 13px; }
  kbd { font: 11px var(--font); padding: 1px 6px; border-radius: 4px; background: var(--md-sys-color-surface-container-highest); }
  .columns { flex: 1; min-height: 0; display: flex; }
  .snackbar {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 6px 6px 16px;
    border-radius: var(--md-sys-shape-corner-small);
    background: #e3e5e8;
    color: #1b1e23;
    z-index: 60;
  }
  .snackbar .icon-btn { color: #1b1e23; }
  .todo { color: var(--md-sys-color-on-surface-variant); }
</style>
