<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import AccountRail from './components/AccountRail.svelte';
  import AddAccount from './components/AddAccount.svelte';
  import CalendarView from './components/CalendarView.svelte';
  import ContactDialog from './components/ContactDialog.svelte';
  import ContactsView from './components/ContactsView.svelte';
  import Compose from './components/Compose.svelte';
  import Dialog from './components/Dialog.svelte';
  import FolderPane from './components/FolderPane.svelte';
  import Icon from './components/Icon.svelte';
  import MessageList from './components/MessageList.svelte';
  import Reader from './components/Reader.svelte';
  import SettingsPane from './components/SettingsPane.svelte';
  import Splitter from './components/Splitter.svelte';
  import { api, pim } from './lib/api';
  import { displayName, longDate } from './lib/format';
  import type {
    Account, ComposeInit, Folder, MessageAction, MessageSummary, MessageView, NotifyMode, Settings, Source, ThreadNode,
  } from './lib/types';

  let accounts = $state<Account[]>([]);
  let accountId = $state<string | null>(null);
  let folders = $state<Folder[]>([]);
  let folderId = $state<string | null>(null);
  let threads = $state<ThreadNode[]>([]);
  let loadingThreads = $state(false);
  let messageId = $state<string | null>(null);
  let message = $state<MessageView | null>(null);
  let loadingMessage = $state(false);
  let settings = $state<Settings | null>(null);
  let view = $state<'mail' | 'calendar' | 'contacts'>('mail');
  let newEventSignal = $state(0);
  let pimSources = $state<Source[]>([]);
  let settingsOpen = $state(false);
  let dialog = $state<null | 'account' | 'contact' | 'reauth'>(null);
  let compose = $state<ComposeInit | null>(null);
  let toast = $state<{ text: string; error?: boolean } | null>(null);
  let query = $state('');
  let searchResults = $state<MessageSummary[] | null>(null);
  let reauthPassword = $state('');
  let reauthTotp = $state('');
  let reauthMailbox = $state('');
  let reauthProton = $state(false);
  let search: HTMLInputElement;

  let foldersWidth = $state(240);
  let listWidth = $state(400);

  const account = $derived(accounts.find((a) => a.id === accountId) ?? null);
  const folder = $derived(findFolder(folders, folderId));

  function findFolder(list: Folder[], id: string | null): Folder | null {
    for (const f of list) {
      if (f.id === id) return f;
      const c = findFolder(f.children, id);
      if (c) return c;
    }
    return null;
  }

  /** Turn the bridge's "needs:" replies into instructions. */
  function friendly(msg: string): string {
    if (msg.includes('needs:totp')) return 'Enter the two-factor code from your authenticator app.';
    if (msg.includes('needs:mailbox_password')) return 'Enter your Proton mailbox password.';
    if (msg.includes('needs:human_verification')) {
      return 'Proton wants to check that you are human. Solve the CAPTCHA that opened in your browser, then sign in again.';
    }
    return msg;
  }

  function notify(text: string, error = false) {
    text = friendly(text);
    toast = { text, error };
    if (!error) setTimeout(() => toast?.text === text && (toast = null), 3500);
  }

  async function guard<T>(p: Promise<T>): Promise<T | undefined> {
    try {
      return await p;
    } catch (e) {
      notify(String(e), true);
    }
  }

  async function refreshAccounts() {
    accounts = (await guard(api.listAccounts())) ?? accounts;
    if (accountId && !accounts.some((a) => a.id === accountId)) {
      accountId = null;
      folders = [];
      folderId = null;
      threads = [];
    }
  }

  async function refreshFolders() {
    if (!accountId) return;
    folders = (await guard(api.listFolders(accountId))) ?? [];
    if (!findFolder(folders, folderId)) {
      folderId = (folders.find((f) => f.role === 'inbox') ?? folders[0])?.id ?? null;
    }
  }

  async function refreshThreads() {
    if (!folderId) {
      threads = [];
      return;
    }
    threads = (await guard(api.listThreads(folderId))) ?? [];
  }

  async function selectAccount(id: string) {
    view = 'mail';
    searchResults = null;
    if (id === accountId && folders.length) return;
    accountId = id;
    folderId = null;
    messageId = null;
    message = null;
    loadingThreads = true;
    await refreshFolders();
    await refreshThreads();
    loadingThreads = false;
  }

  async function selectFolder(f: Folder) {
    searchResults = null;
    folderId = f.id;
    messageId = null;
    message = null;
    loadingThreads = true;
    await refreshThreads();
    loadingThreads = false;
  }

  async function openMessage(m: MessageSummary, allowRemote = false) {
    messageId = m.id;
    if (!allowRemote) message = null;
    loadingMessage = true;
    const v = await guard(api.getMessage(m.id, allowRemote));
    loadingMessage = false;
    if (messageId !== m.id) return;
    message = v ?? null;
    if (m.unread) {
      m.unread = false;
      setTimeout(refreshCounts, 300);
    }
  }

  async function refreshCounts() {
    await refreshAccounts();
    await refreshFolders();
  }

  async function star(m: MessageSummary) {
    m.flagged = !m.flagged;
    await guard(api.setFlag(m.id, 'flagged', m.flagged));
  }

  async function toggleRead(m: MessageSummary) {
    m.unread = !m.unread;
    await guard(api.setFlag(m.id, 'seen', !m.unread));
    refreshCounts();
  }

  function plainBody(v: MessageView): string {
    if (v.message.body_text) return v.message.body_text;
    const doc = new DOMParser().parseFromString(v.body?.html ?? '', 'text/html');
    return doc.body.innerText ?? '';
  }

  function quoted(v: MessageView): string {
    const s = v.message.summary;
    const lines = plainBody(v).trimEnd().split('\n').map((l) => `> ${l}`);
    return `\n\nOn ${longDate(s.date)}, ${displayName(s.from)} wrote:\n${lines.join('\n')}`;
  }

  function stripPrefix(subject: string) {
    return subject.replace(/^((re|fwd?|aw|sv)(\[\d+\])?:\s*)+/i, '');
  }

  async function act(action: MessageAction) {
    const v = message;
    if (!v) return;
    const s = v.message.summary;
    const me = accounts.find((a) => a.id === s.account_id)?.email.toLowerCase();
    switch (action) {
      case 'reply':
      case 'reply_all': {
        let cc = '';
        if (action === 'reply_all') {
          cc = [...v.message.to, ...v.message.cc]
            .filter((a) => a.email.toLowerCase() !== me && a.email.toLowerCase() !== s.from.email.toLowerCase())
            .map((a) => (a.name ? `${a.name} <${a.email}>` : a.email))
            .join(', ');
        }
        compose = {
          account_id: s.account_id,
          to: s.from.name ? `${s.from.name} <${s.from.email}>` : s.from.email,
          cc,
          subject: 'Re: ' + stripPrefix(s.subject),
          text: quoted(v),
          reply_to_id: s.id,
        };
        break;
      }
      case 'forward':
        compose = {
          account_id: s.account_id,
          subject: 'Fwd: ' + stripPrefix(s.subject),
          text: `\n\n---------- Forwarded message ----------\nFrom: ${displayName(s.from)} <${s.from.email}>\nDate: ${longDate(s.date)}\nSubject: ${s.subject}\n\n${plainBody(v)}`,
          forward_id: s.id,
          forwarded_attachments: v.message.attachments.map((a) => a.name),
        };
        break;
      case 'archive':
      case 'delete': {
        const ok = await guard(action === 'archive' ? api.archiveMessage(s.id) : api.deleteMessage(s.id));
        if (ok !== undefined) {
          notify(action === 'archive' ? 'Archived' : 'Moved to Trash');
          message = null;
          messageId = null;
          await refreshThreads();
          refreshCounts();
        }
        break;
      }
      case 'toggle_read':
        s.unread = !s.unread;
        await guard(api.setFlag(s.id, 'seen', !s.unread));
        refreshCounts();
        break;
      case 'toggle_star':
        await star(s);
        break;
    }
  }

  async function moveTo(dest: string) {
    const s = message?.message.summary;
    if (!s) return;
    if ((await guard(api.moveMessage(s.id, dest))) !== undefined) {
      notify(`Moved to ${findFolder(folders, dest)?.name ?? 'folder'}`);
      message = null;
      messageId = null;
      await refreshThreads();
      refreshCounts();
    }
  }

  async function quickReply(text: string) {
    const v = message;
    if (!v) return;
    const s = v.message.summary;
    await api
      .sendMessage({
        account_id: s.account_id,
        to: s.from.email,
        cc: '',
        bcc: '',
        subject: 'Re: ' + stripPrefix(s.subject),
        text: text + quoted(v),
        reply_to_id: s.id,
        attachments: [],
      })
      .then(() => notify('Reply sent'))
      .catch((e) => notify(String(e), true));
  }

  async function runSearch() {
    const q = query.trim();
    if (!q) {
      searchResults = null;
      return;
    }
    view = 'mail';
    searchResults = (await guard(api.search(q, null))) ?? [];
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

  async function reauth() {
    if (!account) return;
    const kind = (await guard(api.accountDetails(account.id)))?.auth.kind;
    if (kind !== 'o_auth2') {
      reauthPassword = '';
      reauthTotp = '';
      reauthMailbox = '';
      reauthProton = kind === 'proton_bridge';
      dialog = 'reauth';
    } else {
      notify('Finish signing in in your browser…');
      if ((await guard(api.reauthenticate(account.id, null))) !== undefined) notify('Signed in');
    }
  }

  async function submitReauth() {
    if (!account) return;
    if ((await guard(api.reauthenticate(account.id, reauthPassword, reauthTotp || null, reauthMailbox || null))) !== undefined) {
      dialog = null;
      notify('Signed in');
    }
  }

  async function addSomething(what: 'account' | 'contact' | 'event') {
    if (what === 'event') {
      view = 'calendar';
      newEventSignal++;
    } else if (what === 'contact') {
      pimSources = (await guard(pim.sources())) ?? [];
      dialog = 'contact';
    } else {
      dialog = 'account';
    }
  }

  async function accountAdded(a: Account) {
    dialog = null;
    await refreshAccounts();
    await selectAccount(a.id);
    notify(`Added ${a.email}. Syncing…`);
  }

  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  function scheduleRefresh(changedAccount?: string) {
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(async () => {
      await refreshAccounts();
      if (!changedAccount || changedAccount === accountId) {
        await refreshFolders();
        if (!searchResults) await refreshThreads();
      }
    }, 250);
  }

  onMount(() => {
    (async () => {
      settings = (await guard(api.getSettings())) ?? null;
      if (settings) {
        foldersWidth = settings.layout.folders_width;
        listWidth = settings.layout.list_width;
      }
      await refreshAccounts();
      if (accounts.length) await selectAccount(accounts[0].id);
    })();
    const unlisteners = [
      listen('compose', () => (compose = { account_id: accountId })),
      listen<{ account_id: string } | null>('mail-changed', (e) => scheduleRefresh(e.payload?.account_id)),
      listen('account-status', () => refreshAccounts()),
      listen<{ account_id: string; folder_id: string; message_id: string }>('open-message', async (e) => {
        const t = e.payload;
        await selectAccount(t.account_id);
        const f = findFolder(folders, t.folder_id);
        if (f) await selectFolder(f);
        if (t.message_id) {
          const all = threads.flatMap(function flat(n: ThreadNode): MessageSummary[] { return [n.message, ...n.children.flatMap(flat)]; });
          const m = all.find((m) => m.id === t.message_id);
          if (m) openMessage(m);
        }
      }),
    ];
    return () => unlisteners.forEach((u) => u.then((f) => f()));
  });

  function keys(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement)?.closest?.('input, textarea, select, [contenteditable]');
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      search.focus();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
      e.preventDefault();
      compose = { account_id: accountId };
    } else if (!typing && !compose && !dialog && !settingsOpen && message) {
      if (e.key === 'r') act('reply');
      else if (e.key === 'a') act('archive');
      else if (e.key === 'Delete') act('delete');
      else if (e.key === 's') act('toggle_star');
    }
  }
</script>

<svelte:window onkeydown={keys} />

<div class="app">
  <header class="topbar">
    <span class="brand"><Icon name="forum" size={20} fill /> Moho Mail</span>
    <form class="search" onsubmit={(e) => { e.preventDefault(); runSearch(); }}>
      <Icon name="search" size={18} />
      <input bind:this={search} bind:value={query} placeholder="Search all mail" oninput={() => !query && (searchResults = null)} />
      <kbd>Ctrl K</kbd>
    </form>
    <span class="spacer"></span>
  </header>

  <div class="columns">
    <AccountRail
      {accounts}
      selected={accountId}
      {view}
      onselect={selectAccount}
      oncalendar={() => (view = view === 'calendar' ? 'mail' : 'calendar')}
      oncontacts={() => (view = view === 'contacts' ? 'mail' : 'contacts')}
      onsettings={() => (settingsOpen = true)}
      onadd={addSomething}
      onnotify={setNotify}
    />

    {#if view === 'calendar'}
      <CalendarView {accounts} {newEventSignal} />
    {:else if view === 'contacts'}
      <ContactsView oncompose={(to) => (compose = { account_id: accountId, to })} />
    {:else if !accounts.length}
      <div class="welcome">
        <Icon name="forum" size={64} fill />
        <h1>Welcome to Moho Mail</h1>
        <p>Add your first account to get started. Gmail, Outlook, Proton, cock.li and any IMAP provider are supported.</p>
        <button class="filled-btn" onclick={() => (dialog = 'account')}><Icon name="person_add" /> Add account</button>
      </div>
    {:else}
      <FolderPane
        {account}
        {folders}
        selected={searchResults ? null : folderId}
        width={foldersWidth}
        onselect={selectFolder}
        oncompose={() => (compose = { account_id: accountId })}
        onsync={() => api.syncNow(accountId)}
        onreauth={reauth}
      />
      <Splitter bind:width={foldersWidth} min={180} max={420} onrelease={persistLayout} />
      <MessageList
        {folder}
        {threads}
        {searchResults}
        searchQuery={query}
        loading={loadingThreads || account?.status.state === 'syncing'}
        selected={messageId}
        width={listWidth}
        onselect={(m) => openMessage(m)}
        onstar={star}
        onread={toggleRead}
        onclearsearch={() => { query = ''; searchResults = null; }}
      />
      <Splitter bind:width={listWidth} min={280} max={720} onrelease={persistLayout} />
      <Reader
        view={message}
        loading={loadingMessage}
        {folders}
        onloadremote={() => message && openMessage(message.message.summary, true)}
        onaction={act}
        onmove={moveTo}
        onquickreply={quickReply}
      />
    {/if}
  </div>

  {#if toast}
    <div class="snackbar" class:error={toast.error} role="alert">
      <span>{toast.text}</span>
      <button class="icon-btn" aria-label="Dismiss" onclick={() => (toast = null)}><Icon name="close" size={18} /></button>
    </div>
  {/if}
</div>

{#if settingsOpen && settings}
  <SettingsPane
    {settings}
    {accounts}
    onchange={saveSettings}
    onnotify={setNotify}
    ontest={(id) => api.testNotification(id)}
    onaccountschanged={refreshAccounts}
    onclose={() => (settingsOpen = false)}
  />
{/if}

{#if dialog === 'account'}
  <AddAccount onclose={() => (dialog = null)} onadded={accountAdded} />
{:else if dialog === 'reauth' && account}
  <Dialog title="Sign in to {account.email}" onclose={() => (dialog = null)}>
    <form class="reauth" onsubmit={(e) => { e.preventDefault(); submitReauth(); }}>
      <p>{account.status.state === 'needs_auth' ? account.status.message : ''}</p>
      <button type="submit" class="implicit-submit" tabindex="-1" aria-hidden="true"></button>
      <input type="password" placeholder="Password" bind:value={reauthPassword} />
      {#if reauthProton}
        <input inputmode="numeric" placeholder="Two-factor code (if enabled)" bind:value={reauthTotp} />
        <input type="password" placeholder="Mailbox password (two-password mode only)" bind:value={reauthMailbox} />
      {/if}
    </form>
    {#snippet actions()}
      <button class="tonal-btn" onclick={() => (dialog = null)}>Cancel</button>
      <button class="filled-btn" onclick={submitReauth}>Sign in</button>
    {/snippet}
  </Dialog>
{:else if dialog === 'contact'}
  <ContactDialog sources={pimSources} onclose={() => (dialog = null)} />
{/if}

{#if compose}
  <Compose
    {accounts}
    init={compose}
    onclose={() => (compose = null)}
    onsent={() => { compose = null; notify('Message sent'); }}
  />
{/if}

<style>
  .app { height: 100%; display: flex; flex-direction: column; }
  .topbar {
    height: 40px; flex: none; display: flex; align-items: center; gap: 16px; padding: 0 12px;
    background: var(--md-sys-color-surface-container-lowest); border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }
  .brand { display: flex; align-items: center; gap: 8px; font-weight: 700; color: var(--md-sys-color-primary); width: 220px; }
  .spacer { width: 220px; }
  .search {
    flex: 0 1 420px; margin: 0 auto; display: flex; align-items: center; gap: 8px; height: 28px; padding: 0 6px 0 10px;
    border-radius: var(--md-sys-shape-corner-small); background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
  }
  .search input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font-size: 13px; }
  kbd { font: 11px var(--font); padding: 1px 6px; border-radius: 4px; background: var(--md-sys-color-surface-container-highest); }
  .columns { flex: 1; min-height: 0; display: flex; }
  .welcome {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px;
    text-align: center; color: var(--md-sys-color-on-surface-variant);
  }
  .welcome :global(.material-symbols-rounded) { color: var(--md-sys-color-primary); }
  .welcome h1 { margin: 8px 0 0; color: var(--md-sys-color-on-surface); font-weight: 600; }
  .welcome p { max-width: 420px; margin: 0 0 12px; }
  .snackbar {
    position: fixed; left: 50%; bottom: 24px; transform: translateX(-50%); max-width: 70vw; display: flex; align-items: center;
    gap: 12px; padding: 6px 6px 6px 16px; border-radius: var(--md-sys-shape-corner-small);
    background: #e3e5e8; color: #1b1e23; z-index: 60; user-select: text;
  }
  .snackbar.error { background: #ffdad6; color: #410002; }
  .snackbar .icon-btn { color: inherit; }
  .reauth { display: flex; flex-direction: column; gap: 10px; }
  .reauth p { margin: 0; font-size: 13px; color: var(--md-sys-color-on-surface-variant); }
  .reauth input {
    height: 42px; padding: 0 12px; border: 1px solid var(--md-sys-color-outline); border-radius: var(--md-sys-shape-corner-small);
    background: transparent; outline: 0;
  }
</style>
