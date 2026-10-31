<script lang="ts">
  import Icon from './Icon.svelte';
  import FolderItem from './FolderItem.svelte';
  import { folderIcon } from '../lib/format';
  import type { Folder } from '../lib/types';

  let {
    folder,
    selected,
    depth = 0,
    nested = true,
    onselect,
    onmenu,
  }: {
    folder: Folder;
    selected: string | null;
    depth?: number;
    /** Show subfolders (off for the flat Favorites list). */
    nested?: boolean;
    onselect: (f: Folder) => void;
    onmenu: (e: MouseEvent, f: Folder) => void;
  } = $props();

  let open = $state(true);
</script>

<div
  class="row"
  class:active={selected === folder.id}
  class:unread={folder.unread > 0}
  style:padding-left="{8 + depth * 16}px"
  role="none"
  oncontextmenu={(e) => onmenu(e, folder)}
>
  {#if nested && folder.children.length}
    <button class="twisty" aria-label={open ? 'Collapse' : 'Expand'} onclick={() => (open = !open)}>
      <Icon name={open ? 'expand_more' : 'chevron_right'} size={16} />
    </button>
  {:else}
    <span class="twisty"></span>
  {/if}
  <button class="name" onclick={() => onselect(folder)} aria-current={selected === folder.id}>
    <Icon name={folderIcon[folder.role]} size={18} fill={selected === folder.id} />
    <span class="label">{folder.name}</span>
    {#if folder.unread > 0}<span class="count">{folder.unread}</span>{/if}
  </button>
</div>
{#if nested && open}
  {#each folder.children as child (child.id)}
    <FolderItem folder={child} {selected} depth={depth + 1} {onselect} {onmenu} />
  {/each}
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    margin: 1px 8px;
    border-radius: var(--md-sys-shape-corner-small);
    color: var(--md-sys-color-on-surface-variant);
  }
  .row:hover { background: var(--md-sys-state-hover); color: var(--md-sys-color-on-surface); }
  .row.active { background: var(--md-sys-state-selected); color: var(--md-sys-color-on-surface); }
  .row.unread { color: var(--md-sys-color-on-surface); font-weight: 600; }
  .twisty { width: 18px; display: grid; place-items: center; flex: none; }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px 6px 2px;
    text-align: left;
  }
  .label { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count { font-size: 12px; color: var(--md-sys-color-on-surface-variant); }
</style>
