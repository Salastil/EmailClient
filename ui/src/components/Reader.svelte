<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import Icon from './Icon.svelte';
  import { api } from '../lib/api';
  import { displayName, fileSize, fullAddress, hashColor, initials, longDate } from '../lib/format';
  import type { Folder, MessageAction, MessageView } from '../lib/types';

  let {
    view,
    loading = false,
    folders,
    onloadremote,
    onaction,
    onmove,
    onquickreply,
  }: {
    view: MessageView | null;
    loading?: boolean;
    folders: Folder[];
    onloadremote: () => void;
    onaction: (a: MessageAction) => void;
    onmove: (folderId: string) => void;
    onquickreply: (text: string) => Promise<void>;
  } = $props();

  let frame = $state<HTMLIFrameElement>();
  let frameHeight = $state(200);
  let quickReply = $state('');
  let sending = $state(false);
  let menu = $state<null | 'more' | 'move'>(null);
  let note = $state<string | null>(null);

  const summary = $derived(view?.message.summary);
  const remoteAllowed = $derived(!!view?.body && view.body.blocked_remote === 0);
  const flatFolders = $derived.by(() => {
    const out: { f: Folder; depth: number }[] = [];
    const walk = (list: Folder[], depth: number) => list.forEach((f) => { out.push({ f, depth }); walk(f.children, depth + 1); });
    walk(folders, 0);
    return out.filter(({ f }) => f.id !== summary?.folder_id);
  });

  /** Email HTML lives in a script-less sandbox with its own CSP. */
  function srcdoc(html: string, allowRemote: boolean): string {
    const img = allowRemote ? 'data: cid: https: http:' : 'data: cid:';
    const csp = `default-src 'none'; img-src ${img}; style-src 'unsafe-inline'; font-src data:`;
    return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<base target="_blank">
<style>
  html { color-scheme: light; }
  body { margin: 0; padding: 20px 24px; background: #fff; color: #1d1d1f;
         font: 14px/1.55 'Noto Sans', system-ui, sans-serif; overflow-wrap: anywhere; }
  img { max-width: 100%; height: auto; }
  pre { white-space: pre-wrap; font: inherit; }
  blockquote { margin: 0 0 0 4px; padding-left: 12px; border-left: 3px solid #c7c7cc; color: #555; }
  a { color: #0b62c4; }
</style></head><body>${html}</body></html>`;
  }

  function escapeHtml(s: string) {
    return s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!);
  }

  /** Plain text with links and quoted lines made readable. */
  function textToHtml(text: string): string {
    const lines = escapeHtml(text).split(/\r?\n/);
    const linked = lines.map((l) => l.replace(/\bhttps?:\/\/[^\s<>"]+/g, (u) => `<a href="${u}">${u}</a>`));
    let html = '';
    let inQuote = false;
    for (const l of linked) {
      const q = l.startsWith('&gt;');
      if (q && !inQuote) html += '<blockquote>';
      if (!q && inQuote) html += '</blockquote>';
      inQuote = q;
      html += (q ? l.replace(/^(&gt;\s?)+/, '') : l) + '\n';
    }
    if (inQuote) html += '</blockquote>';
    return `<pre>${html}</pre>`;
  }

  const doc = $derived(
    view ? srcdoc(view.body?.html ?? textToHtml(view.message.body_text ?? ''), remoteAllowed) : '',
  );

  function onFrameLoad() {
    const d = frame?.contentDocument;
    if (!d) return;
    const fit = () => (frameHeight = Math.max(120, d.documentElement.scrollHeight));
    fit();
    new ResizeObserver(fit).observe(d.body);
    // Links open in the user's default browser, never inside the app.
    d.addEventListener('click', (e) => {
      const a = (e.target as Element).closest?.('a[href]') as HTMLAnchorElement | null;
      if (!a) return;
      e.preventDefault();
      if (/^(https?|mailto):/i.test(a.href)) openUrl(a.href);
    });
  }

  async function sendQuick() {
    if (!quickReply.trim()) return;
    sending = true;
    try {
      await onquickreply(quickReply);
      quickReply = '';
    } finally {
      sending = false;
    }
  }

  async function saveAtt(index: number) {
    if (!summary) return;
    try {
      const path = await api.saveAttachment(summary.id, index);
      if (path) note = `Saved to ${path}`;
    } catch (e) {
      note = String(e);
    }
  }

  async function openAtt(index: number) {
    if (!summary) return;
    try {
      await api.openAttachment(summary.id, index);
    } catch (e) {
      note = String(e);
    }
  }
</script>

<svelte:window onclick={() => (menu = null)} />

<main class="reader" aria-label="Message">
  {#if view && summary}
    <header>
      <h1 title={summary.subject}>{summary.subject || '(no subject)'}</h1>
      <div class="actions">
        <button class="icon-btn" title="Reply (R)" onclick={() => onaction('reply')}><Icon name="reply" /></button>
        <button class="icon-btn" title="Reply all" onclick={() => onaction('reply_all')}><Icon name="reply_all" /></button>
        <button class="icon-btn" title="Forward" onclick={() => onaction('forward')}><Icon name="forward" /></button>
        <button class="icon-btn" title="Archive (A)" onclick={() => onaction('archive')}><Icon name="archive" /></button>
        <button class="icon-btn" title="Delete (Del)" onclick={() => onaction('delete')}><Icon name="delete" /></button>
        <div class="anchor">
          <button class="icon-btn" title="More" onclick={(e) => { e.stopPropagation(); menu = menu ? null : 'more'; }}><Icon name="more_vert" /></button>
          {#if menu === 'more'}
            <div class="menu" role="menu">
              <button role="menuitem" onclick={() => onaction('toggle_read')}><Icon name="mark_email_unread" /> Mark as unread</button>
              <button role="menuitem" onclick={() => onaction('toggle_star')}><Icon name="star" fill={summary.flagged} /> {summary.flagged ? 'Remove star' : 'Star'}</button>
              <button role="menuitem" onclick={(e) => { e.stopPropagation(); menu = 'move'; }}><Icon name="drive_file_move" /> Move to…</button>
            </div>
          {:else if menu === 'move'}
            <div class="menu scroll" role="menu">
              {#each flatFolders as { f, depth }}
                <button role="menuitem" style:padding-left="{10 + depth * 14}px" onclick={() => onmove(f.id)}>{f.name}</button>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    </header>

    <div class="scroll">
      <div class="meta">
        <span class="avatar" style:background={hashColor(summary.from.email)}>{initials(displayName(summary.from))}</span>
        <div class="who">
          <div class="line">
            <span class="from">{fullAddress(summary.from)}</span>
            <span class="date">{longDate(summary.date)}</span>
          </div>
          {#if view.message.to.length}<div class="to">To: {view.message.to.map(fullAddress).join(', ')}</div>{/if}
          {#if view.message.cc.length}<div class="to">Cc: {view.message.cc.map(fullAddress).join(', ')}</div>{/if}
        </div>
      </div>

      {#if view.body && view.body.blocked_remote > 0}
        <div class="banner">
          <Icon name="shield" size={18} />
          <span>{view.body.blocked_remote} remote {view.body.blocked_remote === 1 ? 'image was' : 'images were'} blocked to protect your privacy.</span>
          <button class="tonal-btn" onclick={onloadremote}>Load images</button>
        </div>
      {/if}

      <iframe
        bind:this={frame}
        title="Message body"
        sandbox="allow-same-origin"
        srcdoc={doc}
        style:height="{frameHeight}px"
        onload={onFrameLoad}
      ></iframe>

      {#if view.message.attachments.length}
        <div class="attachments">
          {#each view.message.attachments as att}
            <div class="attachment" title="{att.name} ({att.mime})">
              <button class="att-open" onclick={() => openAtt(att.index)}>
                <Icon name={att.mime.startsWith('image/') ? 'image' : att.mime === 'application/pdf' ? 'picture_as_pdf' : 'draft'} />
                <span class="att-name">{att.name}</span>
                <span class="att-size">{fileSize(att.size)}</span>
              </button>
              <button class="icon-btn" title="Save as…" onclick={() => saveAtt(att.index)}><Icon name="download" size={18} /></button>
            </div>
          {/each}
        </div>
      {/if}
      {#if note}<p class="note">{note}</p>{/if}
    </div>

    <footer>
      <form class="quick" onsubmit={(e) => { e.preventDefault(); sendQuick(); }}>
        <input placeholder="Reply to {displayName(summary.from)}…" bind:value={quickReply} disabled={sending} />
        <button class="icon-btn" title="Send" disabled={!quickReply.trim() || sending}><Icon name="send" /></button>
      </form>
    </footer>
  {:else}
    <div class="empty">
      {#if loading}
        <div class="spinner"></div>
      {:else}
        <Icon name="mail" size={56} />
        <p>Select a message to read it</p>
      {/if}
    </div>
  {/if}
</main>

<style>
  .reader { flex: 1; min-width: 280px; display: flex; flex-direction: column; background: var(--md-sys-color-surface); }
  header { height: 52px; display: flex; align-items: center; gap: 12px; padding: 0 8px 0 20px; border-bottom: 1px solid var(--md-sys-color-outline-variant); }
  h1 { flex: 1; min-width: 0; margin: 0; font-size: 16px; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .actions { display: flex; gap: 2px; }
  .anchor { position: relative; }
  .menu {
    position: absolute; right: 0; top: 36px; z-index: 20; min-width: 220px; padding: 6px;
    border-radius: var(--md-sys-shape-corner-medium); background: var(--md-sys-color-surface-container-highest);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.45); display: flex; flex-direction: column;
  }
  .menu.scroll { max-height: 360px; overflow-y: auto; }
  .menu button { display: flex; align-items: center; gap: 12px; padding: 8px 10px; border-radius: var(--md-sys-shape-corner-small); text-align: left; }
  .menu button:hover { background: var(--md-sys-state-hover); }
  .scroll { flex: 1; overflow-y: auto; padding: 16px 20px; user-select: text; }
  .meta { display: flex; gap: 12px; margin-bottom: 14px; }
  .avatar { width: 40px; height: 40px; flex: none; border-radius: 50%; display: grid; place-items: center; font-weight: 700; color: #fff; }
  .who { flex: 1; min-width: 0; }
  .line { display: flex; gap: 12px; align-items: baseline; flex-wrap: wrap; }
  .from { font-weight: 600; }
  .date, .to { font-size: 12px; color: var(--md-sys-color-on-surface-variant); }
  .banner {
    display: flex; align-items: center; gap: 10px; margin-bottom: 12px; padding: 8px 8px 8px 14px;
    border-radius: var(--md-sys-shape-corner-medium); background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container); font-size: 13px;
  }
  .banner span { flex: 1; }
  iframe { display: block; width: 100%; border: 0; border-radius: var(--md-sys-shape-corner-medium); background: #fff; }
  .attachments { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 14px; }
  .attachment { display: flex; align-items: center; border-radius: var(--md-sys-shape-corner-medium); background: var(--md-sys-color-surface-container-high); padding-right: 4px; }
  .att-open { display: flex; align-items: center; gap: 8px; padding: 8px 8px 8px 12px; }
  .attachment:hover { background: var(--md-sys-color-surface-container-highest); }
  .att-size { font-size: 12px; color: var(--md-sys-color-on-surface-variant); }
  .note { font-size: 12px; color: var(--md-sys-color-on-surface-variant); }
  footer { padding: 10px 16px 14px; }
  .quick { display: flex; align-items: center; gap: 8px; padding: 4px 6px 4px 16px; border-radius: var(--md-sys-shape-corner-medium); background: var(--md-sys-color-surface-container-high); }
  .quick input { flex: 1; border: 0; outline: 0; background: none; height: 36px; }
  .empty { flex: 1; display: grid; place-content: center; justify-items: center; color: var(--md-sys-color-on-surface-variant); }
  .spinner { width: 32px; height: 32px; border-radius: 50%; border: 3px solid var(--md-sys-color-surface-container-highest); border-top-color: var(--md-sys-color-primary); animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
