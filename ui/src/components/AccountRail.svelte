<script lang="ts">
  import Icon from './Icon.svelte';
  import { providerIcon } from '../lib/format';
  import type { Account, NotifyMode } from '../lib/types';

  let {
    accounts,
    selected,
    view,
    onselect,
    oncalendar,
    oncontacts,
    onsettings,
    onadd,
    onnotify,
  }: {
    accounts: Account[];
    selected: string | null;
    view: 'mail' | 'calendar' | 'contacts' | 'settings';
    onselect: (id: string) => void;
    oncalendar: () => void;
    oncontacts: () => void;
    onsettings: () => void;
    onadd: (what: 'account' | 'contact' | 'event') => void;
    onnotify: (id: string, mode: NotifyMode) => void;
  } = $props();

  let addOpen = $state(false);
  let ctx = $state<{ id: string; x: number; y: number } | null>(null);

  const modes: { mode: NotifyMode; label: string; icon: string }[] = [
    { mode: 'all', label: 'All new mail', icon: 'notifications' },
    { mode: 'important_only', label: 'Important only', icon: 'notification_important' },
    { mode: 'muted', label: 'Muted', icon: 'notifications_off' },
  ];

  function contextMenu(e: MouseEvent, id: string) {
    e.preventDefault();
    ctx = { id, x: e.clientX, y: e.clientY };
  }

  function closeMenus() {
    addOpen = false;
    ctx = null;
  }
</script>

<svelte:window onclick={closeMenus} onkeydown={(e) => e.key === 'Escape' && closeMenus()} />

<nav class="rail" aria-label="Accounts">
  <div class="accounts">
    {#each accounts as a (a.id)}
      {@const active = view === 'mail' && selected === a.id}
      <div class="slot" class:active class:syncing={a.status.state === 'syncing'}>
        <span class="pill"></span>
        <button
          class="avatar"
          style:--accent={a.color}
          title={a.email}
          aria-label={a.email}
          aria-current={active}
          onclick={() => onselect(a.id)}
          oncontextmenu={(e) => contextMenu(e, a.id)}
        >
          {#if a.icon}
            <img src={a.icon} alt="" />
          {:else}
            {providerIcon[a.provider] || a.email[0].toUpperCase()}
          {/if}
        </button>
        {#if a.status.state === 'needs_auth' || a.status.state === 'error'}
          <span class="corner alert" title={a.status.message}><Icon name="priority_high" size={12} /></span>
        {:else if a.status.state === 'offline'}
          <span class="corner muted" title="Offline: {a.status.message}"><Icon name="cloud_off" size={12} /></span>
        {:else if a.notify === 'muted'}
          <span class="corner muted" title="Notifications muted"><Icon name="notifications_off" size={12} /></span>
        {:else if a.unread > 0}
          <span class="corner badge">{a.unread > 99 ? '99+' : a.unread}</span>
        {:else if a.notify === 'important_only'}
          <span class="corner important" title="Important mail only"><Icon name="priority_high" size={12} /></span>
        {/if}
      </div>
    {/each}
  </div>

  <div class="tools">
    <hr />
    <div class="menu-anchor">
      <button
        class="tool add"
        title="Add…"
        aria-haspopup="menu"
        aria-expanded={addOpen}
        onclick={(e) => { e.stopPropagation(); ctx = null; addOpen = !addOpen; }}
      >
        <Icon name="add" size={24} />
      </button>
      {#if addOpen}
        <div class="menu up" role="menu">
          <button role="menuitem" onclick={() => onadd('account')}><Icon name="person_add" /> Add account</button>
          <button role="menuitem" onclick={() => onadd('contact')}><Icon name="contact_page" /> New contact</button>
          <button role="menuitem" onclick={() => onadd('event')}><Icon name="event" /> New event</button>
        </div>
      {/if}
    </div>
    <div class="slot" class:active={view === 'calendar'}>
      <span class="pill"></span>
      <button class="tool" title="Calendar" aria-pressed={view === 'calendar'} onclick={oncalendar}>
        <Icon name="calendar_month" size={24} fill={view === 'calendar'} />
      </button>
    </div>
    <div class="slot" class:active={view === 'contacts'}>
      <span class="pill"></span>
      <button class="tool" title="Contacts" aria-pressed={view === 'contacts'} onclick={oncontacts}>
        <Icon name="contacts" size={24} fill={view === 'contacts'} />
      </button>
    </div>
    <div class="slot" class:active={view === 'settings'}>
      <span class="pill"></span>
      <button class="tool" title="Settings" aria-pressed={view === 'settings'} onclick={onsettings}>
        <Icon name="settings" size={24} fill={view === 'settings'} />
      </button>
    </div>
  </div>
</nav>

{#if ctx}
  {@const acct = accounts.find((a) => a.id === ctx!.id)}
  <div class="menu floating" role="menu" style:left="{ctx.x}px" style:top="{ctx.y}px">
    <div class="menu-title">Notifications · {acct?.email}</div>
    {#each modes as m}
      <button role="menuitemradio" aria-checked={acct?.notify === m.mode} onclick={() => onnotify(ctx!.id, m.mode)}>
        <Icon name={m.icon} />
        {m.label}
        {#if acct?.notify === m.mode}<span class="check"><Icon name="check" size={18} /></span>{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .rail {
    flex: 0 0 72px;
    display: flex;
    flex-direction: column;
    background: var(--md-sys-color-surface-container-lowest);
    padding: 8px 0;
  }
  .accounts {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    scrollbar-width: none;
  }
  .slot {
    position: relative;
    display: flex;
    justify-content: center;
  }
  .pill {
    position: absolute;
    left: 0;
    top: 50%;
    width: 4px;
    height: 0;
    border-radius: 0 4px 4px 0;
    background: var(--md-sys-color-on-surface);
    transform: translateY(-50%);
    transition: height 0.15s;
  }
  .slot:hover .pill { height: 20px; }
  .slot.active .pill { height: 40px; }

  .avatar, .tool {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    transition: border-radius 0.15s, background 0.15s;
  }
  .avatar {
    background: var(--md-sys-color-surface-container-high);
    color: var(--accent);
    font-weight: 800;
    font-size: 18px;
  }
  .avatar img { width: 100%; height: 100%; object-fit: cover; border-radius: inherit; }
  .avatar:has(img) { overflow: hidden; }
  .avatar:hover, .active .avatar {
    border-radius: var(--md-sys-shape-corner-large);
    background: var(--accent);
    color: #fff;
  }
  .corner {
    position: absolute;
    right: 4px;
    bottom: -6px;
    border: 3px solid var(--md-sys-color-surface-container-lowest);
    box-sizing: content-box;
  }
  .muted, .important {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface-variant);
  }
  .important { color: var(--md-sys-color-primary); }
  .alert { width: 18px; height: 18px; border-radius: 50%; display: grid; place-items: center; background: var(--md-sys-color-error); color: #fff; }
  .syncing .avatar { box-shadow: 0 0 0 2px var(--md-sys-color-primary); }

  .tools {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding-top: 4px;
  }
  hr {
    width: 32px;
    border: 0;
    border-top: 2px solid var(--md-sys-color-outline-variant);
    margin: 0 0 4px;
  }
  .tool {
    background: var(--md-sys-color-surface-container);
    color: var(--md-sys-color-on-surface-variant);
  }
  .tool:hover, .active .tool {
    border-radius: var(--md-sys-shape-corner-large);
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-primary);
  }
  .tool.add { color: var(--md-sys-color-secondary); border: 1px dashed var(--md-sys-color-outline); }
  .tool.add:hover { background: #1f3a27; }

  .menu-anchor { position: relative; }
  .menu {
    position: absolute;
    z-index: 30;
    min-width: 200px;
    padding: 6px;
    border-radius: var(--md-sys-shape-corner-medium);
    background: var(--md-sys-color-surface-container-highest);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.45);
    display: flex;
    flex-direction: column;
  }
  .menu.up { left: 56px; bottom: 0; }
  .menu.floating { position: fixed; }
  .menu button {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: var(--md-sys-shape-corner-small);
    text-align: left;
  }
  .menu button:hover { background: var(--md-sys-state-hover); }
  .menu-title {
    padding: 6px 10px;
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .check { margin-left: auto; color: var(--md-sys-color-primary); }
</style>
