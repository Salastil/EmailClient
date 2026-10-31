<script lang="ts">
  import Icon from './Icon.svelte';
  import { folderIcon } from '../lib/format';
  import type { Account, FolderRole, NotifyMode, RuleField, Settings } from '../lib/types';

  let {
    settings,
    accounts,
    onchange,
    onnotify,
    ontest,
    onclose,
  }: {
    settings: Settings;
    accounts: Account[];
    onchange: (s: Settings) => void;
    onnotify: (accountId: string, mode: NotifyMode) => void;
    ontest: (accountId: string) => Promise<boolean>;
    onclose: () => void;
  } = $props();

  type Section = 'accounts' | 'notifications' | 'tray' | 'privacy' | 'about';
  const nav: { id: Section; label: string; icon: string }[] = [
    { id: 'accounts', label: 'Accounts', icon: 'manage_accounts' },
    { id: 'notifications', label: 'Notifications', icon: 'notifications' },
    { id: 'tray', label: 'Tray & window', icon: 'select_window' },
    { id: 'privacy', label: 'Privacy', icon: 'shield' },
    { id: 'about', label: 'About', icon: 'info' },
  ];
  let section = $state<Section>('accounts');
  // Edited locally and pushed up on each change.
  // svelte-ignore state_referenced_locally
  let s = $state(structuredClone($state.snapshot(settings)) as Settings);
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
        </div>
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
        {#each [
          ['use_provider_important', 'Marked important by the provider', 'Gmail’s Important label'],
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
    {:else}
      <h2>About Moho Mail</h2>
      <div class="card about">
        <p>Version 0.1.0 · GPL-3.0-or-later</p>
        <p>Rendering with WebKitGTK. Settings are stored in <code>~/.config/moho-mail</code>, caches in <code>~/.cache/moho-mail</code>, and passwords in your system keyring.</p>
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
  code { font-family: var(--mono); font-size: 12px; background: var(--md-sys-color-surface-container-highest); padding: 1px 6px; border-radius: 4px; }
</style>
