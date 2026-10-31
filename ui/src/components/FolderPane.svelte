<script lang="ts">
  import Icon from './Icon.svelte';
  import FolderItem from './FolderItem.svelte';
  import FolderMenu from './FolderMenu.svelte';
  import type { Account, Folder } from '../lib/types';

  let {
    account,
    folders,
    selected,
    width,
    onselect,
    oncompose,
    onsync,
    onreauth,
    favorites,
    ontogglefavorite,
    onsearchin,
    onfolderschanged,
    notify,
  }: {
    account: Account | null;
    folders: Folder[];
    selected: string | null;
    width: number;
    onselect: (f: Folder) => void;
    oncompose: () => void;
    onsync: () => void;
    onreauth: () => void;
    /** Folder ids the user pinned to Favorites. */
    favorites: string[];
    ontogglefavorite: (f: Folder) => void;
    onsearchin: (f: Folder) => void;
    onfolderschanged: (selectId: string | null) => void;
    notify: (text: string, error?: boolean) => void;
  } = $props();

  let menu = $state<{ folder: Folder; x: number; y: number } | null>(null);

  function openMenu(e: MouseEvent, f: Folder) {
    e.preventDefault();
    e.stopPropagation();
    menu = { folder: f, x: e.clientX, y: e.clientY };
  }

  function flat(list: Folder[]): Folder[] {
    return list.flatMap((f) => [f, ...flat(f.children)]);
  }

  // Spin for at least a moment after a click, even when the check is quick.
  let checking = $state(false);
  function checkNow() {
    checking = true;
    setTimeout(() => (checking = false), 1000);
    onsync();
  }

  const status = $derived.by(() => {
    const st = account?.status;
    switch (st?.state) {
      case 'syncing': return { dot: 'sync', text: 'Syncing…', title: '' };
      case 'offline': return { dot: 'off', text: 'Offline', title: st.message };
      case 'error': return { dot: 'err', text: 'Sync problem', title: st.message };
      case 'needs_auth': return { dot: 'err', text: 'Sign-in needed', title: st.message };
      default: return { dot: 'ok', text: 'Up to date', title: '' };
    }
  });

  const FAVORITES = ['inbox', 'starred', 'sent', 'drafts', 'archive', 'spam', 'trash'];
  let sections = $derived([
    {
      key: 'favorites',
      title: 'Favorites',
      items: [
        ...folders.filter((f) => FAVORITES.includes(f.role)),
        ...flat(folders).filter((f) => !FAVORITES.includes(f.role) && favorites.includes(f.id)),
      ],
    },
    { key: 'folders', title: 'Folders', items: folders.filter((f) => f.role === 'other') },
    { key: 'labels', title: 'Labels', items: folders.filter((f) => f.role === 'label') },
  ].filter((s) => s.items.length));

  let collapsed = $state<Record<string, boolean>>({});
</script>

<aside class="pane" style:width="{width}px" aria-label="Folders">
  <header>
    <div class="title">
      <span class="name">{account?.email ?? ''}</span>
    </div>
    <button class="icon-btn" title="Compose" onclick={oncompose}><Icon name="edit_square" /></button>
  </header>

  <div class="list">
    {#each sections as s (s.key)}
      <button class="section" onclick={() => (collapsed[s.key] = !collapsed[s.key])} aria-expanded={!collapsed[s.key]}>
        <Icon name={collapsed[s.key] ? 'chevron_right' : 'expand_more'} size={14} />
        {s.title}
      </button>
      {#if !collapsed[s.key]}
        {#each s.items as f (f.id)}
          <FolderItem folder={f} {selected} {onselect} onmenu={openMenu} nested={s.key !== 'favorites' || FAVORITES.includes(f.role)} />
        {/each}
      {/if}
    {/each}
  </div>

  {#if menu}
    {#key menu}
      <FolderMenu
        folder={menu.folder}
        {folders}
        {account}
        x={menu.x}
        y={menu.y}
        favorite={favorites.includes(menu.folder.id)}
        pinnedByRole={FAVORITES.includes(menu.folder.role)}
        onclose={() => (menu = null)}
        onsearch={onsearchin}
        {ontogglefavorite}
        onchanged={onfolderschanged}
        {notify}
      />
    {/key}
  {/if}

  {#if account}
    {#if account.status.state === 'needs_auth'}
      <button class="reauth" onclick={onreauth}><Icon name="key" size={18} /> Sign in again</button>
    {/if}
    <footer>
      <span class="me" style:background={account.color}>{#if account.icon}<img src={account.icon} alt="" />{:else}{account.email[0].toUpperCase()}{/if}</span>
      <div class="who">
        <span class="name">{account.email}</span>
        <span class="status" title={status.title}><span class="dot {status.dot}"></span>{status.text}</span>
      </div>
      <button class="icon-btn" title="Check for new mail" onclick={checkNow}>
        <span class:spin={checking || status.dot === 'sync'}><Icon name="sync" size={18} /></span>
      </button>
    </footer>
  {/if}
</aside>

<style>
  .pane {
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--md-sys-color-surface-container);
    min-width: 0;
  }
  header {
    height: 52px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px 0 16px;
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }
  .title { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .title .name { font-weight: 700; }
  .who .status {
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .list { flex: 1; overflow-y: auto; padding: 8px 0; }
  .section {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 12px 12px 4px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--md-sys-color-on-surface-variant);
  }
  .section:hover { color: var(--md-sys-color-on-surface); }
  footer {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    background: var(--md-sys-color-surface-container-low);
  }
  .me img { width: 100%; height: 100%; object-fit: cover; border-radius: inherit; }
  .me {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-weight: 700;
    color: #fff;
  }
  .who { flex: 1; display: flex; flex-direction: column; min-width: 0; }
  .who .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot.sync { background: var(--md-sys-color-primary); }
  .dot.off { background: #9aa1a9; }
  .dot.err { background: var(--md-sys-color-error); }
  .spin { display: inline-block; animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(-360deg); } }
  .reauth {
    display: flex; align-items: center; justify-content: center; gap: 8px; margin: 8px 12px; height: 36px;
    border-radius: var(--md-sys-shape-corner-full); background: var(--md-sys-color-error); color: #fff; font-weight: 600;
  }
  .who .name { font-weight: 600; font-size: 13px; }
  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #3ba55d;
    margin-right: 6px;
  }
</style>
