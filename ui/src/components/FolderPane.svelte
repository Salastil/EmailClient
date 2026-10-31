<script lang="ts">
  import Icon from './Icon.svelte';
  import FolderItem from './FolderItem.svelte';
  import type { Account, Folder } from '../lib/types';

  let {
    account,
    folders,
    selected,
    width,
    onselect,
    oncompose,
  }: {
    account: Account | null;
    folders: Folder[];
    selected: string | null;
    width: number;
    onselect: (f: Folder) => void;
    oncompose: () => void;
  } = $props();

  const FAVORITES = ['inbox', 'starred', 'sent', 'drafts', 'archive', 'spam', 'trash'];
  let sections = $derived([
    { key: 'favorites', title: 'Favorites', items: folders.filter((f) => FAVORITES.includes(f.role)) },
    { key: 'folders', title: 'Folders', items: folders.filter((f) => f.role === 'other') },
    { key: 'labels', title: 'Labels', items: folders.filter((f) => f.role === 'label') },
  ].filter((s) => s.items.length));

  let collapsed = $state<Record<string, boolean>>({});
</script>

<aside class="pane" style:width="{width}px" aria-label="Folders">
  <header>
    <div class="title">
      <span class="name">{account?.name ?? ''}</span>
      <span class="email">{account?.email ?? ''}</span>
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
          <FolderItem folder={f} {selected} {onselect} />
        {/each}
      {/if}
    {/each}
  </div>

  {#if account}
    <footer>
      <span class="me" style:background={account.color}>{account.email[0].toUpperCase()}</span>
      <div class="who">
        <span class="name">{account.email.split('@')[0]}</span>
        <span class="status"><span class="dot"></span>Online · synced</span>
      </div>
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
  .title .email, .who .status {
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
  .me {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-weight: 700;
    color: #fff;
  }
  .who { display: flex; flex-direction: column; min-width: 0; }
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
