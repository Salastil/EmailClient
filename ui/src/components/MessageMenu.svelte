<script lang="ts">
  import Icon from './Icon.svelte';
  import { folderIcon } from '../lib/format';
  import type { Folder, MessageMenuAction as MenuAction, MessageSummary } from '../lib/types';

  let {
    message,
    folders,
    x,
    y,
    inThread,
    onaction,
    onmove,
    oncopy,
    onclose,
  }: {
    message: MessageSummary;
    /** The message's account folder tree, for Move to / Copy to. */
    folders: Folder[];
    x: number;
    y: number;
    /** The message belongs to a thread with other messages. */
    inThread: boolean;
    onaction: (a: MenuAction) => void;
    onmove: (folderId: string) => void;
    oncopy: (folderId: string) => void;
    onclose: () => void;
  } = $props();

  let submenu = $state<'move' | 'copy' | 'mark' | null>(null);
  let menuEl = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0 });

  // Keep the menu on screen.
  $effect(() => {
    if (!menuEl) return;
    const r = menuEl.getBoundingClientRect();
    pos = { left: Math.min(x, innerWidth - r.width - 8), top: Math.max(8, Math.min(y, innerHeight - r.height - 8)) };
  });
  const flyLeft = $derived(pos.left + 560 > innerWidth);

  function flatten(list: Folder[], depth = 0): { f: Folder; depth: number }[] {
    return list.flatMap((f) => [{ f, depth }, ...flatten(f.children, depth + 1)]);
  }
  // Gmail's All Mail and Starred are views; messages are not moved into them.
  const targets = $derived(flatten(folders).filter(({ f }) => f.role !== 'all' && f.role !== 'starred'));
  const hasSpam = $derived(targets.some(({ f }) => f.role === 'spam'));
  const inSpam = $derived(targets.find(({ f }) => f.id === message.folder_id)?.f.role === 'spam');

  function act(a: MenuAction) {
    onaction(a);
    onclose();
  }

  const quick: { action: MenuAction; icon: string; label: string }[] = [
    { action: 'reply', icon: 'reply', label: 'Reply' },
    { action: 'reply_all', icon: 'reply_all', label: 'Reply all' },
    { action: 'forward', icon: 'forward', label: 'Forward' },
    { action: 'archive', icon: 'archive', label: 'Archive' },
    { action: 'junk', icon: 'report', label: 'Mark as junk' },
    { action: 'delete', icon: 'delete', label: 'Delete' },
  ];
</script>

<svelte:window
  onclick={onclose}
  oncontextmenu={onclose}
  onkeydown={(e) => e.key === 'Escape' && onclose()}
  onblur={onclose}
  onresize={onclose}
/>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="menu"
  role="menu"
  tabindex="-1"
  aria-label="Message"
  bind:this={menuEl}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  onclick={(e) => e.stopPropagation()}
  oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); }}
>
  <div class="quick" role="group" aria-label="Quick actions">
    {#each quick as q}
      <button
        role="menuitem"
        class="icon-btn"
        title={q.label}
        aria-label={q.label}
        disabled={q.action === 'junk' && (!hasSpam || inSpam)}
        onmouseenter={() => (submenu = null)}
        onclick={() => act(q.action)}
      >
        <Icon name={q.icon} size={20} />
      </button>
    {/each}
  </div>
  <hr />
  <button role="menuitem" onmouseenter={() => (submenu = null)} onclick={() => act('open')}>
    <Icon name="mail" size={18} /> Open
  </button>
  <button role="menuitem" onmouseenter={() => (submenu = null)} onclick={() => act('edit_as_new')}>
    <Icon name="edit_note" size={18} /> Edit as new message
  </button>
  <hr />
  <div class="sub" role="none" onmouseenter={() => (submenu = 'move')}>
    <button role="menuitem" aria-haspopup="menu" aria-expanded={submenu === 'move'} onclick={() => (submenu = 'move')}>
      <Icon name="drive_file_move" size={18} /> Move to <span class="end"><Icon name="chevron_right" size={18} /></span>
    </button>
    {#if submenu === 'move'}{@render folderList('move')}{/if}
  </div>
  <div class="sub" role="none" onmouseenter={() => (submenu = 'copy')}>
    <button role="menuitem" aria-haspopup="menu" aria-expanded={submenu === 'copy'} onclick={() => (submenu = 'copy')}>
      <Icon name="content_copy" size={18} /> Copy to <span class="end"><Icon name="chevron_right" size={18} /></span>
    </button>
    {#if submenu === 'copy'}{@render folderList('copy')}{/if}
  </div>
  <div class="sub" role="none" onmouseenter={() => (submenu = 'mark')}>
    <button role="menuitem" aria-haspopup="menu" aria-expanded={submenu === 'mark'} onclick={() => (submenu = 'mark')}>
      <Icon name="done_all" size={18} /> Mark <span class="end"><Icon name="chevron_right" size={18} /></span>
    </button>
    {#if submenu === 'mark'}
      <div class="menu flyout" class:left={flyLeft} role="menu" aria-label="Mark">
        <button role="menuitem" onclick={() => act('toggle_read')}>
          <Icon name={message.unread ? 'drafts' : 'mark_email_unread'} size={18} /> {message.unread ? 'As read' : 'As unread'}
        </button>
        {#if inThread}
          <button role="menuitem" onclick={() => act('thread_read')}><Icon name="done_all" size={18} /> Thread as read</button>
        {/if}
        <button role="menuitem" onclick={() => act('toggle_star')}>
          <Icon name="star" size={18} fill={!message.flagged} /> {message.flagged ? 'Remove star' : 'As starred'}
        </button>
        <button role="menuitem" disabled={!hasSpam || inSpam} onclick={() => act('junk')}>
          <Icon name="report" size={18} /> As junk
        </button>
      </div>
    {/if}
  </div>
  <button role="menuitem" onmouseenter={() => (submenu = null)} onclick={() => act('archive')}>
    <Icon name="archive" size={18} /> Archive
  </button>
  <hr />
  <button role="menuitem" onmouseenter={() => (submenu = null)} onclick={() => act('save')}>
    <Icon name="download" size={18} /> Save as…
  </button>
  <hr />
  <button role="menuitem" class="danger" onmouseenter={() => (submenu = null)} onclick={() => act('delete')}>
    <Icon name="delete" size={18} /> Delete
  </button>
</div>

{#snippet folderList(kind: 'move' | 'copy')}
  <div class="menu flyout" class:left={flyLeft} role="menu" aria-label={kind === 'move' ? 'Move to' : 'Copy to'}>
    {#each targets as { f, depth } (f.id)}
      <button
        role="menuitem"
        style:padding-left="{10 + depth * 16}px"
        disabled={f.id === message.folder_id}
        onclick={() => { (kind === 'move' ? onmove : oncopy)(f.id); onclose(); }}
      >
        <Icon name={folderIcon[f.role]} size={18} /> <span class="label">{f.name}</span>
      </button>
    {/each}
  </div>
{/snippet}

<style>
  .menu {
    position: fixed;
    z-index: 45;
    min-width: 240px;
    padding: 6px;
    border-radius: var(--md-sys-shape-corner-medium);
    background: var(--md-sys-color-surface-container-highest);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.45);
    display: flex;
    flex-direction: column;
    outline: 0;
  }
  .menu > button, .sub > button, .flyout button {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 7px 10px;
    border-radius: var(--md-sys-shape-corner-small);
    text-align: left;
    white-space: nowrap;
  }
  .menu button:hover:not(:disabled), .menu button[aria-expanded='true'] { background: var(--md-sys-state-hover); }
  .menu button:disabled { opacity: 0.4; cursor: default; }
  .menu hr { width: 100%; margin: 4px 0; border: 0; border-top: 1px solid var(--md-sys-color-outline-variant); }
  .quick { display: flex; justify-content: space-between; padding: 0 2px; }
  .quick .icon-btn { width: 34px; height: 34px; padding: 0; justify-content: center; }
  .end { margin-left: auto; display: grid; }
  .sub { position: relative; display: flex; flex-direction: column; }
  .flyout {
    position: absolute;
    left: calc(100% + 4px);
    top: -6px;
    max-height: min(420px, 70vh);
    max-width: 320px;
    overflow-y: auto;
  }
  .flyout.left { left: auto; right: calc(100% + 4px); }
  .label { overflow: hidden; text-overflow: ellipsis; }
  .danger { color: var(--md-sys-color-error); }
</style>
