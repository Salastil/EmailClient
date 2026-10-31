<script lang="ts">
  import AddressInput from './AddressInput.svelte';
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
  import { api } from '../lib/api';
  import type { Account, ComposeInit } from '../lib/types';

  let {
    accounts,
    init,
    onclose,
    onsent,
  }: { accounts: Account[]; init: ComposeInit; onclose: () => void; onsent: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let f = $state({
    account_id: init.account_id ?? accounts[0]?.id ?? '',
    to: init.to ?? '',
    cc: init.cc ?? '',
    bcc: '',
    subject: init.subject ?? '',
    text: init.text ?? '',
  });
  // svelte-ignore state_referenced_locally
  let showCc = $state(!!init.cc);
  let attachments = $state<string[]>([]);
  let sending = $state(false);
  let error = $state<string | null>(null);

  async function attach() {
    const picked = await api.pickAttachments().catch((e) => { error = String(e); return []; });
    attachments = [...attachments, ...picked.filter((p) => !attachments.includes(p))];
  }

  async function send() {
    error = null;
    sending = true;
    try {
      await api.sendMessage({
        ...f,
        reply_to_id: init.reply_to_id ?? null,
        forward_id: init.forward_id ?? null,
        attachments,
      });
      onsent();
    } catch (e) {
      error = String(e);
    } finally {
      sending = false;
    }
  }

  const base = (p: string) => p.split('/').pop();
</script>

<svelte:window onkeydown={(e) => { if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') send(); }} />

<Dialog title={init.reply_to_id ? 'Reply' : init.forward_id && !init.edit_as_new ? 'Forward' : 'New message'} {onclose} width={760}>
  <div class="fields">
    <label class="row"><span>From</span>
      <select bind:value={f.account_id}>
        {#each accounts as a}<option value={a.id}>{a.email}</option>{/each}
      </select>
    </label>
    <div class="row"><span>To</span><AddressInput bind:value={f.to} />
      {#if !showCc}<button class="tiny" onclick={() => (showCc = true)}>Cc/Bcc</button>{/if}
    </div>
    {#if showCc}
      <div class="row"><span>Cc</span><AddressInput bind:value={f.cc} /></div>
      <div class="row"><span>Bcc</span><AddressInput bind:value={f.bcc} /></div>
    {/if}
    <label class="row"><span>Subject</span><input bind:value={f.subject} /></label>
    <textarea bind:value={f.text} rows="16" placeholder="Write your message…"></textarea>
    {#if attachments.length || init.forwarded_attachments?.length}
      <div class="atts">
        {#each init.forwarded_attachments ?? [] as name}
          <span class="att"><Icon name="attach_file" size={16} />{name}</span>
        {/each}
        {#each attachments as p, i}
          <span class="att" title={p}><Icon name="attach_file" size={16} />{base(p)}
            <button aria-label="Remove" onclick={() => attachments.splice(i, 1)}><Icon name="close" size={14} /></button>
          </span>
        {/each}
      </div>
    {/if}
    {#if error}<p class="error"><Icon name="error" size={18} /> {error}</p>{/if}
  </div>
  {#snippet actions()}
    <button class="icon-btn" title="Attach files" onclick={attach}><Icon name="attach_file" /></button>
    <span style="flex:1"></span>
    <button class="tonal-btn" onclick={onclose}>Discard</button>
    <button class="filled-btn" disabled={sending || !f.account_id} onclick={send} title="Send (Ctrl+Enter)">
      <Icon name="send" size={18} /> {sending ? 'Sending…' : 'Send'}
    </button>
  {/snippet}
</Dialog>

<style>
  .fields { display: flex; flex-direction: column; gap: 8px; }
  .row { display: flex; align-items: center; gap: 12px; }
  .row > span { width: 60px; flex: none; font-size: 13px; color: var(--md-sys-color-on-surface-variant); }
  /* Filled like the body below, so each field stands out from the dialog. */
  .row input, .row select {
    flex: 1; min-width: 0; height: 38px; padding: 0 12px; outline: 0;
    border: 1px solid transparent; border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-surface-container);
  }
  .row input:focus, .row select:focus { border-color: var(--md-sys-color-primary); }
  .row select option { background: var(--md-sys-color-surface-container-high); }
  .tiny { font-size: 12px; color: var(--md-sys-color-primary); }
  textarea {
    margin-top: 4px; resize: vertical; border: 1px solid transparent; outline: 0; min-height: 240px;
    background: var(--md-sys-color-surface-container); border-radius: var(--md-sys-shape-corner-medium); padding: 12px;
    font: 14px/1.5 var(--font);
  }
  textarea:focus { border-color: var(--md-sys-color-primary); }
  .atts { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px; }
  .att { display: inline-flex; align-items: center; gap: 6px; padding: 4px 6px 4px 10px; border-radius: var(--md-sys-shape-corner-small); background: var(--md-sys-color-surface-container-highest); font-size: 13px; }
  .error { display: flex; gap: 8px; color: #ffb4ab; font-size: 13px; margin: 10px 0 0; }
</style>
