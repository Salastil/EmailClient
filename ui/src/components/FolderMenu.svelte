<script lang="ts">
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
  import { folderIcon } from '../lib/format';
  import { api } from '../lib/api';
  import type { Account, Folder } from '../lib/types';

  let {
    folder,
    folders,
    account,
    x,
    y,
    favorite,
    pinnedByRole,
    onclose,
    onsearch,
    ontogglefavorite,
    onchanged,
    notify,
  }: {
    folder: Folder;
    /** The account's whole folder tree. */
    folders: Folder[];
    account: Account | null;
    x: number;
    y: number;
    favorite: boolean;
    /** Special folders always show under Favorites. */
    pinnedByRole: boolean;
    onclose: () => void;
    onsearch: (f: Folder) => void;
    ontogglefavorite: (f: Folder) => void;
    /** Folders changed on the server; select `selectId` when given. */
    onchanged: (selectId: string | null) => void;
    notify: (text: string, error?: boolean) => void;
  } = $props();

  type DialogKind = 'new' | 'rename' | 'delete' | 'properties';
  let menuOpen = $state(true);
  let submenu = $state<'move' | 'copy' | null>(null);
  let dialog = $state<DialogKind | null>(null);
  let name = $state('');
  let busy = $state(false);
  let menuEl = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0 });

  // Folders the user made; special folders (Inbox, Sent, Trash, ...) stay where the server put them.
  const editable = $derived(folder.role === 'other' || folder.role === 'label');
  // Gmail's All Mail and Starred are views, not real folders.
  const virtual = $derived(folder.role === 'all' || folder.role === 'starred');

  // Keep the menu on screen.
  $effect(() => {
    if (!menuEl) return;
    const r = menuEl.getBoundingClientRect();
    pos = { left: Math.min(x, innerWidth - r.width - 8), top: Math.min(y, innerHeight - r.height - 8) };
  });

  function flatten(list: Folder[], depth = 0): { f: Folder; depth: number }[] {
    return list.flatMap((f) => [{ f, depth }, ...flatten(f.children, depth + 1)]);
  }

  function inside(candidate: Folder, root: Folder): boolean {
    return candidate.id === root.id || root.children.some((c) => inside(candidate, c));
  }

  function parentOf(list: Folder[], target: Folder, parent: Folder | null = null): Folder | null | undefined {
    for (const f of list) {
      if (f.id === target.id) return parent;
      const p = parentOf(f.children, target, f);
      if (p !== undefined) return p;
    }
    return undefined;
  }

  /** Where the folder can be moved or copied to. */
  const destinations = $derived(
    flatten(folders).filter(({ f }) => f.role !== 'all' && f.role !== 'starred' && !inside(f, folder)),
  );
  const currentParent = $derived(parentOf(folders, folder) ?? null);

  // Mirrors the backend: Gmail labels are removed, other folders go to Trash
  // unless they are already in it or there is no Trash.
  const trash = $derived(folders.find((f) => f.role === 'trash') ?? null);
  const permanent = $derived(account?.provider !== 'gmail' && (!trash || (inside(folder, trash) && folder.id !== trash.id)));
  // Open submenus to the left when there is no room on the right.
  const flyLeft = $derived(pos.left + 560 > innerWidth);

  function open(kind: DialogKind) {
    menuOpen = false;
    name = kind === 'rename' ? folder.name : '';
    dialog = kind;
  }

  async function run<T>(p: Promise<T>, done: string): Promise<T | undefined> {
    busy = true;
    try {
      const v = await p;
      if (done) notify(done);
      return v;
    } catch (e) {
      notify(String(e), true);
    } finally {
      busy = false;
    }
  }

  async function createSubfolder() {
    if (!name.trim() || !account) return;
    const id = await run(api.createFolder(account.id, folder.id, name), `Created “${name.trim()}”`);
    if (id !== undefined) finish(id);
  }

  async function rename() {
    if (!name.trim()) return;
    const id = await run(api.renameFolder(folder.id, name), '');
    if (id !== undefined) finish(id);
  }

  async function remove() {
    const r = await run(api.deleteFolder(folder.id), '');
    if (r === undefined) return;
    notify(r === 'moved_to_trash' ? `Moved “${folder.name}” to Trash` : `Deleted “${folder.name}”`);
    finish(null);
  }

  async function moveOrCopy(kind: 'move' | 'copy', dest: Folder | null) {
    menuOpen = false;
    const where = dest ? `“${dest.name}”` : 'the top level';
    const id =
      kind === 'move'
        ? await run(api.moveFolder(folder.id, dest?.id ?? null), `Moved “${folder.name}” to ${where}`)
        : await run(api.copyFolder(folder.id, dest?.id ?? null), `Copied “${folder.name}” to ${where}`);
    if (id !== undefined) finish(kind === 'move' ? id : null);
    else onclose();
  }

  async function markRead() {
    menuOpen = false;
    await run(api.markFolderRead(folder.id), '');
    onclose();
  }

  function finish(selectId: string | null) {
    dialog = null;
    onchanged(selectId);
    onclose();
  }

  function closeAll() {
    if (!busy) onclose();
  }
</script>

<svelte:window
  onclick={() => menuOpen && closeAll()}
  oncontextmenu={() => menuOpen && closeAll()}
  onkeydown={(e) => e.key === 'Escape' && menuOpen && closeAll()}
  onblur={() => menuOpen && closeAll()}
/>

{#if menuOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="menu"
    role="menu"
    tabindex="-1"
    aria-label="{folder.name} folder"
    bind:this={menuEl}
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    onclick={(e) => e.stopPropagation()}
    oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); }}
  >
    <button role="menuitem" onmouseenter={() => (submenu = null)} onclick={() => { menuOpen = false; onsearch(folder); onclose(); }}>
      <Icon name="search" size={18} /> Search messages…
    </button>
    <hr />
    {#if !virtual}
      <button role="menuitem" onmouseenter={() => (submenu = null)} onclick={() => open('new')}>
        <Icon name="create_new_folder" size={18} /> New subfolder…
      </button>
    {/if}
    {#if editable}
      <button role="menuitem" onmouseenter={() => (submenu = null)} onclick={() => open('rename')}>
        <Icon name="drive_file_rename_outline" size={18} /> Rename…
      </button>
      <button role="menuitem" onmouseenter={() => (submenu = null)} onclick={() => open('delete')}>
        <Icon name="delete" size={18} /> Delete…
      </button>
    {/if}
    {#if editable || !virtual}
      <hr />
    {/if}
    {#if editable}
      <div class="sub" role="none" onmouseenter={() => (submenu = 'move')}>
        <button role="menuitem" aria-haspopup="menu" aria-expanded={submenu === 'move'} onclick={() => (submenu = 'move')}>
          <Icon name="drive_file_move" size={18} /> Move to <span class="chev"><Icon name="chevron_right" size={18} /></span>
        </button>
        {#if submenu === 'move'}
          {@render targets('move')}
        {/if}
      </div>
    {/if}
    {#if !virtual}
      <div class="sub" role="none" onmouseenter={() => (submenu = 'copy')}>
        <button role="menuitem" aria-haspopup="menu" aria-expanded={submenu === 'copy'} onclick={() => (submenu = 'copy')}>
          <Icon name="content_copy" size={18} /> Copy to <span class="chev"><Icon name="chevron_right" size={18} /></span>
        </button>
        {#if submenu === 'copy'}
          {@render targets('copy')}
        {/if}
      </div>
      <hr />
    {/if}
    <button role="menuitem" disabled={folder.unread === 0} onmouseenter={() => (submenu = null)} onclick={markRead}>
      <Icon name="mark_email_read" size={18} /> Mark folder read
    </button>
    {#if !pinnedByRole}
      <button role="menuitemcheckbox" aria-checked={favorite} onmouseenter={() => (submenu = null)} onclick={() => { menuOpen = false; ontogglefavorite(folder); onclose(); }}>
        <Icon name="star" size={18} fill={favorite} /> Favorite folder
        {#if favorite}<span class="chev"><Icon name="check" size={18} /></span>{/if}
      </button>
    {/if}
    <hr />
    <button role="menuitem" onmouseenter={() => (submenu = null)} onclick={() => open('properties')}>
      <Icon name="info" size={18} /> Properties
    </button>
  </div>
{/if}

{#snippet targets(kind: 'move' | 'copy')}
  <div class="menu flyout" class:left={flyLeft} role="menu" aria-label={kind === 'move' ? 'Move to' : 'Copy to'}>
    <button
      role="menuitem"
      disabled={kind === 'move' && currentParent === null}
      onclick={() => moveOrCopy(kind, null)}
    >
      <Icon name="account_tree" size={18} /> Top level
    </button>
    <hr />
    {#each destinations as { f, depth } (f.id)}
      <button
        role="menuitem"
        style:padding-left="{10 + depth * 16}px"
        disabled={kind === 'move' && currentParent?.id === f.id}
        onclick={() => moveOrCopy(kind, f)}
      >
        <Icon name={folderIcon[f.role]} size={18} /> <span class="label">{f.name}</span>
      </button>
    {/each}
  </div>
{/snippet}

{#if dialog === 'new' || dialog === 'rename'}
  <Dialog title={dialog === 'new' ? `New folder in “${folder.name}”` : `Rename “${folder.name}”`} width={420} onclose={closeAll}>
    <form onsubmit={(e) => { e.preventDefault(); dialog === 'new' ? createSubfolder() : rename(); }}>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="text" placeholder="Folder name" bind:value={name} autofocus disabled={busy} />
    </form>
    {#snippet actions()}
      <button class="tonal-btn" onclick={closeAll}>Cancel</button>
      <button class="filled-btn" disabled={busy || !name.trim()} onclick={() => (dialog === 'new' ? createSubfolder() : rename())}>
        {dialog === 'new' ? 'Create' : 'Rename'}
      </button>
    {/snippet}
  </Dialog>
{:else if dialog === 'delete'}
  <Dialog title="Delete “{folder.name}”?" width={440} onclose={closeAll}>
    <p class="body">
      {#if account?.provider === 'gmail'}
        The label{folder.children.length ? ' and its sublabels are' : ' is'} removed. Messages stay in All Mail.
      {:else if permanent}
        The folder{folder.children.length ? ', its subfolders' : ''} and {folder.children.length ? 'all the messages in them' : folder.total === 1 ? 'its message' : `its ${folder.total} messages`}
        will be deleted from the server for good. This can't be undone.
      {:else}
        The folder{folder.children.length ? ', its subfolders' : ''} and {folder.children.length ? 'all the messages in them' : folder.total === 1 ? 'its message' : `its ${folder.total} messages`}
        move to Trash.
      {/if}
    </p>
    {#snippet actions()}
      <button class="tonal-btn" onclick={closeAll}>Cancel</button>
      <button class="filled-btn danger" disabled={busy} onclick={remove}>Delete</button>
    {/snippet}
  </Dialog>
{:else if dialog === 'properties'}
  <Dialog title="{folder.name} properties" width={440} onclose={closeAll}>
    <dl class="props">
      <dt>Name</dt><dd>{folder.name}</dd>
      <dt>Account</dt><dd>{account?.email ?? ''}</dd>
      <dt>Location on server</dt><dd><code>{folder.path}</code></dd>
      <dt>Type</dt><dd>{folder.role === 'other' ? 'Folder' : folder.role === 'label' ? 'Label' : `Special folder (${folder.role})`}</dd>
      <dt>Messages</dt><dd>{folder.total}</dd>
      <dt>Unread</dt><dd>{folder.unread}</dd>
      <dt>Subfolders</dt><dd>{folder.children.length}</dd>
    </dl>
    {#snippet actions()}
      <button class="filled-btn" onclick={closeAll}>Close</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 45;
    min-width: 230px;
    padding: 6px;
    border-radius: var(--md-sys-shape-corner-medium);
    background: var(--md-sys-color-surface-container-highest);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.45);
    display: flex;
    flex-direction: column;
    outline: 0;
  }
  .menu button {
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
  .chev { margin-left: auto; display: grid; }
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
  .text {
    width: 100%;
    height: 38px;
    padding: 0 12px;
    border-radius: var(--md-sys-shape-corner-small);
    border: 1px solid var(--md-sys-color-outline);
    background: var(--md-sys-color-surface-container-high);
    outline: 0;
  }
  .text:focus { border-color: var(--md-sys-color-primary); }
  .body { margin: 0; color: var(--md-sys-color-on-surface-variant); line-height: 1.5; }
  .danger { background: var(--md-sys-color-error); color: var(--md-sys-color-on-error); }
  .props { display: grid; grid-template-columns: max-content 1fr; gap: 8px 16px; margin: 0; }
  .props dt { color: var(--md-sys-color-on-surface-variant); }
  .props dd { margin: 0; overflow-wrap: anywhere; }
</style>
