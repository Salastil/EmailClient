<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import Icon from './Icon.svelte';
  import { displayName, fileSize, fullAddress, hashColor, initials, longDate } from '../lib/format';
  import type { MessageView } from '../lib/types';

  let {
    view,
    onloadremote,
    onreply,
  }: {
    view: MessageView | null;
    onloadremote: () => void;
    onreply: (kind: 'reply' | 'reply_all' | 'forward') => void;
  } = $props();

  let frame = $state<HTMLIFrameElement>();
  let frameHeight = $state(200);
  let quickReply = $state('');

  const summary = $derived(view?.message.summary);
  const remoteAllowed = $derived(!!view?.body && view.body.blocked_remote === 0);

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
  pre { white-space: pre-wrap; }
  a { color: #0b62c4; }
</style></head><body>${html}</body></html>`;
  }

  function textToHtml(text: string): string {
    const esc = text.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!);
    return `<pre style="font:inherit">${esc}</pre>`;
  }

  const doc = $derived(
    view
      ? srcdoc(view.body?.html ?? textToHtml(view.message.body_text ?? ''), remoteAllowed)
      : '',
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
</script>

<main class="reader" aria-label="Message">
  {#if view && summary}
    <header>
      <h1 title={summary.subject}>{summary.subject}</h1>
      <div class="actions">
        <button class="icon-btn" title="Reply" onclick={() => onreply('reply')}><Icon name="reply" /></button>
        <button class="icon-btn" title="Reply all" onclick={() => onreply('reply_all')}><Icon name="reply_all" /></button>
        <button class="icon-btn" title="Forward" onclick={() => onreply('forward')}><Icon name="forward" /></button>
        <button class="icon-btn" title="Archive"><Icon name="archive" /></button>
        <button class="icon-btn" title="Delete"><Icon name="delete" /></button>
        <button class="icon-btn" title="More"><Icon name="more_vert" /></button>
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
          <div class="to">To: {view.message.to.map(fullAddress).join(', ')}</div>
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
            <button class="attachment" title="{att.name} ({att.mime})">
              <Icon name={att.mime.startsWith('image/') ? 'image' : att.mime === 'application/pdf' ? 'picture_as_pdf' : 'draft'} />
              <span class="att-name">{att.name}</span>
              <span class="att-size">{fileSize(att.size)}</span>
              <Icon name="download" size={18} />
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <footer>
      <label class="quick">
        <input placeholder="Reply to {displayName(summary.from)}…" bind:value={quickReply} />
        <button class="icon-btn" title="Send" disabled={!quickReply.trim()}><Icon name="send" /></button>
      </label>
    </footer>
  {:else}
    <div class="empty">
      <Icon name="mail" size={56} />
      <p>Select a message to read it</p>
    </div>
  {/if}
</main>

<style>
  .reader {
    flex: 1;
    min-width: 280px;
    display: flex;
    flex-direction: column;
    background: var(--md-sys-color-surface);
  }
  header {
    height: 52px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 8px 0 20px;
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }
  h1 {
    flex: 1;
    min-width: 0;
    margin: 0;
    font-size: 16px;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions { display: flex; gap: 2px; }
  .scroll { flex: 1; overflow-y: auto; padding: 16px 20px; user-select: text; }
  .meta { display: flex; gap: 12px; margin-bottom: 14px; }
  .avatar {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-weight: 700;
    color: #fff;
  }
  .who { flex: 1; min-width: 0; }
  .line { display: flex; gap: 12px; align-items: baseline; flex-wrap: wrap; }
  .from { font-weight: 600; }
  .date, .to { font-size: 12px; color: var(--md-sys-color-on-surface-variant); }
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
    padding: 8px 8px 8px 14px;
    border-radius: var(--md-sys-shape-corner-medium);
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    font-size: 13px;
  }
  .banner span { flex: 1; }
  iframe {
    display: block;
    width: 100%;
    border: 0;
    border-radius: var(--md-sys-shape-corner-medium);
    background: #fff;
  }
  .attachments { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 14px; }
  .attachment {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: var(--md-sys-shape-corner-medium);
    background: var(--md-sys-color-surface-container-high);
  }
  .attachment:hover { background: var(--md-sys-color-surface-container-highest); }
  .att-size { font-size: 12px; color: var(--md-sys-color-on-surface-variant); }
  footer { padding: 10px 16px 14px; }
  .quick {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 4px 16px;
    border-radius: var(--md-sys-shape-corner-medium);
    background: var(--md-sys-color-surface-container-high);
  }
  .quick input { flex: 1; border: 0; outline: 0; background: none; height: 36px; }
  .empty {
    flex: 1;
    display: grid;
    place-content: center;
    justify-items: center;
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
