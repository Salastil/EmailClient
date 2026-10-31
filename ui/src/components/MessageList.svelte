<script lang="ts">
  import Icon from './Icon.svelte';
  import MessageCard from './MessageCard.svelte';
  import ThreadBranch from './ThreadBranch.svelte';
  import type { Folder, MessageSummary, ThreadNode } from '../lib/types';

  let {
    folder,
    threads,
    selected,
    width,
    onselect,
    onstar,
  }: {
    folder: Folder | null;
    threads: ThreadNode[];
    selected: string | null;
    width: number;
    onselect: (m: MessageSummary) => void;
    onstar: (m: MessageSummary) => void;
  } = $props();

  let expanded = $state<Record<string, boolean>>({});
  let filterOpen = $state(false);
  let query = $state('');
  let unreadOnly = $state(false);
  let starredOnly = $state(false);
  let newestFirst = $state(true);

  const flatten = (n: ThreadNode): MessageSummary[] => [n.message, ...n.children.flatMap(flatten)];
  const total = $derived(threads.reduce((sum, t) => sum + 1 + t.reply_count, 0));

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = threads.filter((t) => {
      const msgs = flatten(t);
      return (
        (!unreadOnly || msgs.some((m) => m.unread)) &&
        (!starredOnly || msgs.some((m) => m.flagged)) &&
        (!q ||
          msgs.some((m) =>
            `${m.subject} ${m.from.name ?? ''} ${m.from.email}`.toLowerCase().includes(q),
          ))
      );
    });
    return newestFirst ? list : [...list].reverse();
  });
</script>

<section class="list-pane" style:width="{width}px" aria-label="Messages">
  <header>
    <span class="folder">{folder?.name ?? ''}</span>
    <span class="count">{total} messages</span>
    <span class="spacer"></span>
    <button class="icon-btn" class:on={filterOpen} title="Quick filter" onclick={() => (filterOpen = !filterOpen)}>
      <Icon name="filter_list" />
    </button>
    <button class="icon-btn" title={newestFirst ? 'Newest first' : 'Oldest first'} onclick={() => (newestFirst = !newestFirst)}>
      <Icon name={newestFirst ? 'arrow_downward' : 'arrow_upward'} />
    </button>
  </header>

  {#if filterOpen}
    <div class="filter">
      <label class="search">
        <Icon name="search" size={18} />
        <input placeholder="Filter these messages…" bind:value={query} />
      </label>
      <button class="chip" class:on={unreadOnly} onclick={() => (unreadOnly = !unreadOnly)}>
        <Icon name="mark_email_unread" size={16} /> Unread
      </button>
      <button class="chip" class:on={starredOnly} onclick={() => (starredOnly = !starredOnly)}>
        <Icon name="star" size={16} /> Starred
      </button>
    </div>
  {/if}

  <div class="scroll" role="listbox" aria-label="Threads">
    {#each visible as t (t.message.id)}
      <MessageCard
        message={t.message}
        selected={selected === t.message.id}
        replies={t.reply_count}
        expanded={!!expanded[t.message.id]}
        onselect={() => onselect(t.message)}
        ontoggle={() => (expanded[t.message.id] = !expanded[t.message.id])}
        onstar={() => onstar(t.message)}
      />
      {#if expanded[t.message.id] && t.children.length}
        <ThreadBranch nodes={t.children} {selected} {onselect} {onstar} />
      {/if}
    {:else}
      <div class="empty">
        <Icon name="inbox" size={40} />
        <p>Nothing here</p>
      </div>
    {/each}
  </div>
</section>

<style>
  .list-pane {
    flex: none;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--md-sys-color-surface-container-low);
  }
  header {
    height: 52px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px 0 16px;
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }
  .folder { font-weight: 700; }
  .count { font-size: 12px; color: var(--md-sys-color-on-surface-variant); white-space: nowrap; }
  .spacer { flex: 1; }
  .icon-btn.on { color: var(--md-sys-color-primary); background: var(--md-sys-state-selected); }
  .filter {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px;
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }
  .search {
    flex: 1 1 100%;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    height: 32px;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface-variant);
  }
  .search input { flex: 1; border: 0; outline: 0; background: none; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 28px;
    padding: 0 10px;
    border-radius: var(--md-sys-shape-corner-small);
    border: 1px solid var(--md-sys-color-outline);
    font-size: 12px;
  }
  .chip.on {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    border-color: transparent;
  }
  .scroll { flex: 1; overflow-y: auto; padding: 4px 0 12px; }
  .empty {
    display: grid;
    place-items: center;
    padding: 48px 0;
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
