<script lang="ts">
  import Icon from './Icon.svelte';
  import { folderIcon } from '../lib/format';
  import { api } from '../lib/api';
  import type { Account, AccountConfig, FolderRole, NotifyMode, RuleField, Settings } from '../lib/types';

  let {
    settings,
    accounts,
    onchange,
    onnotify,
    ontest,
    onaccountschanged,
    onclose,
  }: {
    settings: Settings;
    accounts: Account[];
    onchange: (s: Settings) => void;
    onnotify: (accountId: string, mode: NotifyMode) => void;
    ontest: (accountId: string) => Promise<boolean>;
    onaccountschanged: () => void;
    onclose: () => void;
  } = $props();

  let editing = $state<{ id: string; name: string; sender_name: string; color: string; config: AccountConfig } | null>(null);
  let confirmRemove = $state<string | null>(null);
  let accountError = $state<string | null>(null);

  async function edit(a: Account) {
    accountError = null;
    const config = await api.accountDetails(a.id);
    editing = { id: a.id, name: config.name, sender_name: config.sender_name, color: config.color, config };
  }

  async function saveAccount() {
    if (!editing) return;
    try {
      await api.updateAccount(editing.id, { name: editing.name, sender_name: editing.sender_name, color: editing.color });
      editing = null;
      onaccountschanged();
    } catch (e) {
      accountError = String(e);
    }
  }

  async function removeAccount(id: string) {
    try {
      await api.removeAccount(id);
      confirmRemove = null;
      editing = null;
      onaccountschanged();
    } catch (e) {
      accountError = String(e);
    }
  }

  // Where to register a client with each provider, and what to create there.
  // Sign-in redirects to http://127.0.0.1:<any port>/ (localhost for Microsoft).
  const OAUTH_PROVIDERS = [
    {
      key: 'google', name: 'Google', secret: true,
      links: [
        { label: 'Create client', url: 'https://console.cloud.google.com/apis/credentials' },
        { label: 'Enable Gmail API', url: 'https://console.cloud.google.com/apis/library/gmail.googleapis.com' },
        { label: 'Enable CalDAV API', url: 'https://console.cloud.google.com/apis/library/caldav.googleapis.com' },
        { label: 'Enable CardDAV API', url: 'https://console.cloud.google.com/apis/library/carddav.googleapis.com' },
      ],
      how: 'Create credentials → OAuth client ID → application type “Desktop app”. In the same project, enable the Gmail API, plus CalDAV and CardDAV for calendars and contacts.',
    },
    {
      key: 'microsoft', name: 'Microsoft', secret: false,
      links: [{ label: 'App registrations', url: 'https://entra.microsoft.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade' }],
      how: 'New registration → add platform “Mobile and desktop applications” with redirect URI http://localhost. No secret is needed.',
    },
    {
      key: 'yahoo', name: 'Yahoo', secret: true,
      links: [{ label: 'Create app', url: 'https://developer.yahoo.com/apps/create/' }],
      how: 'Create an app with Mail read/write access. Yahoo may not offer Mail access to new apps; an app password works instead.',
    },
    {
      key: 'aol', name: 'AOL', secret: true,
      links: [{ label: 'Create app', url: 'https://developer.yahoo.com/apps/create/' }],
      how: 'AOL accounts sign in through Yahoo, so register the client in Yahoo’s developer console as above.',
    },
    {
      key: 'fastmail', name: 'Fastmail', secret: false,
      links: [{ label: 'Fastmail OAuth guide', url: 'https://www.fastmail.com/dev/#o-auth-at-fastmail' }],
      how: 'Fastmail has no self-serve console. Desktop apps can sign in without registering, but Moho Mail doesn’t support that yet; the guide explains manual registration.',
    },
  ];

  const security = (x: string) => ({ tls: 'SSL/TLS', starttls: 'STARTTLS', none: 'none' })[x] ?? x;

  type Section = 'accounts' | 'notifications' | 'tray' | 'privacy' | 'advanced' | 'about';
  const nav: { id: Section; label: string; icon: string }[] = [
    { id: 'accounts', label: 'Accounts', icon: 'manage_accounts' },
    { id: 'notifications', label: 'Notifications', icon: 'notifications' },
    { id: 'tray', label: 'Tray & window', icon: 'select_window' },
    { id: 'privacy', label: 'Privacy', icon: 'shield' },
    { id: 'advanced', label: 'Advanced', icon: 'tune' },
    { id: 'about', label: 'About', icon: 'info' },
  ];
  let section = $state<Section>('accounts');
  // Edited locally and pushed up on each change.
  // svelte-ignore state_referenced_locally
  let s = $state(structuredClone($state.snapshot(settings)) as Settings);
  // Fill in a row per provider here: the template may only read state, and
  // writing a missing entry while rendering blanked the Advanced section.
  for (const p of OAUTH_PROVIDERS) s.oauth_clients[p.key] ??= { client_id: '', client_secret: null };
  let vipInput = $state('');
  let rule = $state<{ field: RuleField; contains: string }>({ field: 'sender', contains: '' });
  let testResult = $state<Record<string, string>>({});

  const commit = () => onchange($state.snapshot(s) as Settings);

  const MUTABLE_FOLDERS: FolderRole[] = ['spam', 'trash', 'sent', 'drafts', 'archive'];
  const modes: { mode: NotifyMode; label: string }[] = [
    { mode: 'all', label: 'All new mail' },
    { mode: 'important_only', label: 'Important only' },
    { mode: 'muted', label: 'Muted' },
  ];

  function toggleFolder(role: FolderRole) {
    const set = new Set(s.notifications.muted_folders);
    set.has(role) ? set.delete(role) : set.add(role);
    s.notifications.muted_folders = [...set];
    commit();
  }

  async function test(id: string) {
    try {
      testResult[id] = (await ontest(id)) ? 'Notification shown' : 'Suppressed by your settings';
    } catch (e) {
      testResult[id] = `Failed: ${e}`;
    }
  }

  const hours = Array.from({ length: 24 }, (_, h) => h);
  const hourLabel = (h: number) => new Date(2000, 0, 1, h).toLocaleTimeString([], { hour: 'numeric' });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="settings" role="dialog" aria-modal="true" aria-label="Settings">
  <nav>
    <div class="nav-title">Settings</div>
    {#each nav as n}
      <button class:on={section === n.id} onclick={() => (section = n.id)}><Icon name={n.icon} /> {n.label}</button>
    {/each}
  </nav>

  <div class="body">
    <button class="close icon-btn" aria-label="Close settings" onclick={onclose}><Icon name="close" size={24} /></button>

    {#if section === 'accounts'}
      <h2>Accounts</h2>
      {#if !accounts.length}<p class="hint">No accounts yet. Use the + button in the left rail to add one.</p>{/if}
      {#each accounts as a (a.id)}
        <div class="card account">
          <span class="glyph" style:background={a.color}>{a.name[0]}</span>
          <div class="grow"><b>{a.name}</b><small>{a.email}</small></div>
          <label class="select">
            <Icon name={a.notify === 'muted' ? 'notifications_off' : a.notify === 'important_only' ? 'notification_important' : 'notifications'} size={18} />
            <select value={a.notify} onchange={(e) => onnotify(a.id, (e.currentTarget as HTMLSelectElement).value as NotifyMode)}>
              {#each modes as m}<option value={m.mode}>{m.label}</option>{/each}
            </select>
          </label>
          <button class="icon-btn" title="Edit" onclick={() => edit(a)}><Icon name="edit" /></button>
        </div>
        {#if editing?.id === a.id}
          <div class="card editor">
            <label class="field">Account name<input class="text" bind:value={editing.name} /></label>
            <label class="field">Your name (From header)<input class="text" bind:value={editing.sender_name} /></label>
            <label class="field">Colour<input type="color" bind:value={editing.color} /></label>
            <p class="hint">
              Incoming {editing.config.imap.host}:{editing.config.imap.port} ({security(editing.config.imap.security)}) ·
              Outgoing {editing.config.smtp.host}:{editing.config.smtp.port} ({security(editing.config.smtp.security)}) ·
              Sign-in: {editing.config.auth.kind === 'o_auth2' ? 'browser (OAuth)' : editing.config.auth.kind === 'proton_bridge' ? 'Proton bridge' : 'password in system keyring'}
            </p>
            <div class="inline">
              {#if confirmRemove === a.id}
                <span class="grow">Remove {a.email} and its cached mail from this computer?</span>
                <button class="tonal-btn" onclick={() => (confirmRemove = null)}>Keep</button>
                <button class="danger-btn" onclick={() => removeAccount(a.id)}>Remove</button>
              {:else}
                <button class="danger-btn" onclick={() => (confirmRemove = a.id)}><Icon name="delete" size={18} /> Remove account</button>
                <span class="grow"></span>
                <button class="tonal-btn" onclick={() => (editing = null)}>Cancel</button>
                <button class="filled-btn" onclick={saveAccount}>Save</button>
              {/if}
            </div>
            {#if accountError}<p class="err">{accountError}</p>{/if}
          </div>
        {/if}
      {/each}
    {:else if section === 'notifications'}
      <h2>Notifications</h2>
      <div class="card">
        <label class="switch-row">
          <div class="grow"><b>Do not disturb</b><small>Silence every notification until turned off</small></div>
          <input type="checkbox" class="switch" bind:checked={s.notifications.do_not_disturb} onchange={commit} />
        </label>
        <label class="switch-row">
          <div class="grow"><b>Quiet hours</b><small>No notifications during these hours</small></div>
          <input
            type="checkbox"
            class="switch"
            checked={!!s.notifications.quiet_hours}
            onchange={(e) => {
              s.notifications.quiet_hours = (e.currentTarget as HTMLInputElement).checked ? [22, 7] : null;
              commit();
            }}
          />
        </label>
        {#if s.notifications.quiet_hours}
          <div class="inline">
            From
            <select bind:value={s.notifications.quiet_hours[0]} onchange={commit}>
              {#each hours as h}<option value={h}>{hourLabel(h)}</option>{/each}
            </select>
            until
            <select bind:value={s.notifications.quiet_hours[1]} onchange={commit}>
              {#each hours as h}<option value={h}>{hourLabel(h)}</option>{/each}
            </select>
          </div>
        {/if}
      </div>

      <h3>Per account</h3>
      <div class="card">
        {#each accounts as a (a.id)}
          <div class="acct-row">
            <span class="glyph small" style:background={a.color}>{a.name[0]}</span>
            <span class="grow">{a.name}</span>
            <div class="segmented">
              {#each modes as m}
                <button class:on={a.notify === m.mode} onclick={() => onnotify(a.id, m.mode)}>{m.label}</button>
              {/each}
            </div>
            <button class="icon-btn" title="Send a test notification" onclick={() => test(a.id)}><Icon name="science" /></button>
          </div>
          {#if testResult[a.id]}<div class="test">{testResult[a.id]}</div>{/if}
        {/each}
      </div>

      <h3>What counts as important</h3>
      <div class="card">
        <div class="switch-row">
          <div class="grow">
            <b>In a folder monitored as important</b>
            <small>Right-click a folder and choose Monitor as important. Unless you change it, that's the Inbox, or Important on Gmail. The tray icon shows unread mail in these folders.</small>
          </div>
        </div>
        {#each [
          ['use_contacts', 'From someone in my contacts', ''],
          ['use_flagged', 'Starred or flagged', ''],
          ['use_replied_threads', 'In a conversation I replied to', ''],
        ] as [key, label, hint]}
          <label class="switch-row">
            <div class="grow"><b>{label}</b>{#if hint}<small>{hint}</small>{/if}</div>
            <input
              type="checkbox"
              class="switch"
              bind:checked={(s.notifications as unknown as Record<string, boolean>)[key]}
              onchange={commit}
            />
          </label>
        {/each}

        <div class="sub">VIP senders</div>
        <div class="chips">
          {#each s.notifications.vip as v, i}
            <span class="chip">{v}<button aria-label="Remove {v}" onclick={() => { s.notifications.vip.splice(i, 1); commit(); }}><Icon name="close" size={14} /></button></span>
          {/each}
        </div>
        <form class="inline" onsubmit={(e) => { e.preventDefault(); if (vipInput.includes('@')) { s.notifications.vip.push(vipInput.trim().toLowerCase()); vipInput = ''; commit(); } }}>
          <input class="text" placeholder="name@example.com" bind:value={vipInput} />
          <button class="tonal-btn">Add VIP</button>
        </form>

        <div class="sub">Rules</div>
        {#each s.notifications.rules as r, i}
          <div class="inline">
            <span class="grow">{r.field} contains “{r.contains}”</span>
            <button class="icon-btn" aria-label="Remove rule" onclick={() => { s.notifications.rules.splice(i, 1); commit(); }}><Icon name="delete" size={18} /></button>
          </div>
        {/each}
        <form class="inline" onsubmit={(e) => { e.preventDefault(); if (rule.contains.trim()) { s.notifications.rules.push({ ...rule }); rule.contains = ''; commit(); } }}>
          <select bind:value={rule.field}>
            <option value="sender">Sender</option>
            <option value="domain">Domain</option>
            <option value="subject">Subject</option>
          </select>
          contains
          <input class="text" bind:value={rule.contains} />
          <button class="tonal-btn">Add rule</button>
        </form>
      </div>

      <h3>Never notify for these folders</h3>
      <div class="card chips">
        {#each MUTABLE_FOLDERS as role}
          <button class="filter-chip" class:on={s.notifications.muted_folders.includes(role)} onclick={() => toggleFolder(role)}>
            <Icon name={s.notifications.muted_folders.includes(role) ? 'check' : folderIcon[role]} size={16} /> {role}
          </button>
        {/each}
      </div>
    {:else if section === 'tray'}
      <h2>Tray & window</h2>
      <div class="card">
        <label class="switch-row">
          <div class="grow"><b>Close to tray</b><small>The close button hides Moho Mail; quit from the tray menu</small></div>
          <input type="checkbox" class="switch" bind:checked={s.tray.close_to_tray} onchange={commit} />
        </label>
        <label class="switch-row">
          <div class="grow"><b>Minimize to tray</b><small>Minimizing hides the window instead of leaving it in the taskbar</small></div>
          <input type="checkbox" class="switch" bind:checked={s.tray.minimize_to_tray} onchange={commit} />
        </label>
        <label class="switch-row">
          <div class="grow"><b>Start when I log in</b><small>Adds Moho Mail to your desktop's autostart</small></div>
          <input type="checkbox" class="switch" bind:checked={s.tray.autostart} onchange={commit} />
        </label>
        <label class="switch-row">
          <div class="grow"><b>Start hidden in the tray</b><small>Useful with autostart</small></div>
          <input type="checkbox" class="switch" bind:checked={s.tray.start_minimized} onchange={commit} />
        </label>
      </div>
      <p class="hint">Click the tray icon to show or hide the window. Right-click it for Compose, pausing notifications and Quit.</p>
    {:else if section === 'privacy'}
      <h2>Privacy</h2>
      <div class="card">
        <label class="switch-row">
          <div class="grow"><b>Always load remote images</b><small>Remote images can tell senders when and where you opened a message</small></div>
          <input type="checkbox" class="switch" bind:checked={s.privacy.load_remote_images} onchange={commit} />
        </label>
      </div>
    {:else if section === 'advanced'}
      <h2>Advanced</h2>
      <h3>OAuth clients</h3>
      <p class="hint">Browser sign-in needs an OAuth client registered with each provider. Builds may include one; to use your own, register it with the provider using the links below and paste its client ID here.</p>
      <div class="card">
        {#each OAUTH_PROVIDERS as p}
          {@const c = s.oauth_clients[p.key]}
          <div class="oauth-provider">
            <div class="oauth-row">
              <b>{p.name}</b>
              <input class="text grow-input" placeholder="Client ID" bind:value={c.client_id} onchange={commit} />
              {#if p.secret}
                <input class="text" placeholder="Client secret (if any)" value={c.client_secret ?? ''} onchange={(e) => { c.client_secret = (e.currentTarget as HTMLInputElement).value || null; commit(); }} />
              {/if}
            </div>
            <div class="oauth-help">
              {#each p.links as l}
                <button class="ext-link" title={l.url} onclick={() => api.openLink(l.url)}>{l.label} <Icon name="open_in_new" size={14} /></button>
              {/each}
              <small>{p.how}</small>
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <h2>About Moho Mail</h2>
      <div class="card about">
        {#await api.appInfo() then info}
          <p>Version {info.version} · GPL-3.0-or-later · rendering with WebKitGTK</p>
          <p>Proton Mail: {info.proton_bridge ? `built-in bridge ${info.proton_bridge}` : 'not included in this build'}</p>
          <p>Settings <code>{info.config_dir}</code></p>
          <p>Cache (safe to delete) <code>{info.cache_dir}</code></p>
          <p>Local calendars, contacts and the Proton vault <code>{info.data_dir}</code></p>
          <p>Passwords and sign-in tokens are kept in your system keyring.</p>
        {/await}
      </div>
    {/if}
  </div>
</div>

<style>
  .settings {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    background: var(--md-sys-color-surface);
    user-select: text;
  }
  nav {
    width: 260px;
    padding: 48px 12px 12px 24px;
    background: var(--md-sys-color-surface-container);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav-title { font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--md-sys-color-on-surface-variant); padding: 0 12px 8px; }
  nav button { display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-radius: var(--md-sys-shape-corner-full); text-align: left; }
  nav button:hover { background: var(--md-sys-state-hover); }
  nav button.on { background: var(--md-sys-color-primary-container); color: var(--md-sys-color-on-primary-container); font-weight: 600; }
  .body { position: relative; flex: 1; overflow-y: auto; padding: 48px 56px; max-width: 900px; }
  .close { position: fixed; top: 20px; right: 24px; width: 40px; height: 40px; border: 1px solid var(--md-sys-color-outline); }
  h2 { margin: 0 0 20px; font-size: 22px; font-weight: 600; }
  h3 { margin: 28px 0 10px; font-size: 13px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--md-sys-color-on-surface-variant); }
  .card { background: var(--md-sys-color-surface-container); border-radius: var(--md-sys-shape-corner-large); padding: 8px 16px; margin-bottom: 10px; }
  .account { display: flex; align-items: center; gap: 14px; padding: 12px 16px; }
  .glyph { width: 40px; height: 40px; flex: none; border-radius: 12px; display: grid; place-items: center; font-weight: 800; color: #fff; }
  .glyph.small { width: 28px; height: 28px; border-radius: 8px; font-size: 13px; }
  .grow { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  small, .hint { color: var(--md-sys-color-on-surface-variant); font-size: 12px; }
  .select { display: flex; align-items: center; gap: 6px; }
  select, .text {
    height: 34px;
    padding: 0 10px;
    border-radius: var(--md-sys-shape-corner-small);
    border: 1px solid var(--md-sys-color-outline);
    background: var(--md-sys-color-surface-container-high);
    outline: 0;
  }
  .switch-row { display: flex; align-items: center; gap: 16px; padding: 10px 0; }
  .switch-row + .switch-row { border-top: 1px solid var(--md-sys-color-outline-variant); }
  .switch {
    appearance: none;
    width: 52px;
    height: 32px;
    flex: none;
    border-radius: 16px;
    border: 2px solid var(--md-sys-color-outline);
    background: var(--md-sys-color-surface-container-highest);
    position: relative;
    cursor: pointer;
    transition: background 0.15s;
  }
  .switch::after {
    content: '';
    position: absolute;
    top: 6px;
    left: 6px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--md-sys-color-outline);
    transition: all 0.15s;
  }
  .switch:checked { background: var(--md-sys-color-primary); border-color: var(--md-sys-color-primary); }
  .switch:checked::after { left: 22px; top: 2px; width: 24px; height: 24px; background: var(--md-sys-color-on-primary); }
  .inline { display: flex; align-items: center; gap: 8px; padding: 8px 0; flex-wrap: wrap; }
  .acct-row { display: flex; align-items: center; gap: 12px; padding: 8px 0; }
  .test { font-size: 12px; color: var(--md-sys-color-primary); padding: 0 0 6px 40px; }
  .segmented { display: flex; border: 1px solid var(--md-sys-color-outline); border-radius: var(--md-sys-shape-corner-full); overflow: hidden; }
  .segmented button { padding: 6px 12px; font-size: 12px; }
  .segmented button + button { border-left: 1px solid var(--md-sys-color-outline); }
  .segmented button.on { background: var(--md-sys-color-primary-container); color: var(--md-sys-color-on-primary-container); font-weight: 600; }
  .sub { margin-top: 14px; font-weight: 600; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; padding: 8px 0; }
  .card.chips { padding: 12px 16px; }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 4px 6px 4px 12px; border-radius: var(--md-sys-shape-corner-small); background: var(--md-sys-color-surface-container-highest); font-size: 13px; }
  .filter-chip { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px; border: 1px solid var(--md-sys-color-outline); border-radius: var(--md-sys-shape-corner-small); text-transform: capitalize; }
  .filter-chip.on { background: var(--md-sys-color-primary-container); color: var(--md-sys-color-on-primary-container); border-color: transparent; }
  .about p { margin: 8px 0; }
  .editor { display: flex; flex-direction: column; gap: 10px; padding: 14px 16px; margin-top: -6px; }
  .field { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--md-sys-color-on-surface-variant); }
  .field input[type='color'] { width: 64px; height: 32px; border: 0; background: none; }
  .danger-btn { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 14px; border-radius: var(--md-sys-shape-corner-full); background: var(--md-sys-color-error); color: #fff; font-weight: 600; }
  .err { color: #ffb4ab; font-size: 13px; }
  .oauth-provider { padding: 8px 0; }
  .oauth-provider + .oauth-provider { border-top: 1px solid var(--md-sys-color-outline-variant); }
  .oauth-row { display: flex; align-items: center; gap: 10px; }
  .oauth-help { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 12px; margin: 6px 0 0 100px; }
  .oauth-help small { flex-basis: 100%; }
  .ext-link { display: inline-flex; align-items: center; gap: 4px; font-size: 13px; color: var(--md-sys-color-primary); }
  .ext-link:hover { text-decoration: underline; }
  .oauth-row b { width: 90px; flex: none; }
  .grow-input { flex: 1; }
  code { font-family: var(--mono); font-size: 12px; background: var(--md-sys-color-surface-container-highest); padding: 1px 6px; border-radius: 4px; }
</style>
