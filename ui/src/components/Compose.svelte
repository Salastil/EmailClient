<script lang="ts">
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
  import type { Account } from '../lib/types';

  let {
    from,
    to = '',
    subject = '',
    onclose,
  }: { from: Account | null; to?: string; subject?: string; onclose: () => void } = $props();

  let fields = $state({ to: '', cc: '', subject: '', body: '' });
  $effect.pre(() => {
    fields.to = to;
    fields.subject = subject;
  });
</script>

<Dialog title="New message" {onclose} width={720}>
  <div class="fields">
    <div class="row"><span>From</span><b>{from ? `${from.name} <${from.email}>` : ''}</b></div>
    <label class="row"><span>To</span><input bind:value={fields.to} /></label>
    <label class="row"><span>Cc</span><input bind:value={fields.cc} /></label>
    <label class="row"><span>Subject</span><input bind:value={fields.subject} /></label>
    <textarea bind:value={fields.body} rows="14" placeholder="Write your message…"></textarea>
  </div>
  {#snippet actions()}
    <button class="icon-btn" title="Attach"><Icon name="attach_file" /></button>
    <span style="flex:1"></span>
    <button class="tonal-btn" onclick={onclose}>Discard</button>
    <button class="filled-btn" disabled title="Sending arrives with SMTP support (milestone 3)"><Icon name="send" size={18} /> Send</button>
  {/snippet}
</Dialog>

<style>
  .fields { display: flex; flex-direction: column; }
  .row { display: flex; align-items: center; gap: 12px; border-bottom: 1px solid var(--md-sys-color-outline-variant); min-height: 40px; }
  .row span { width: 60px; font-size: 13px; color: var(--md-sys-color-on-surface-variant); }
  .row input { flex: 1; height: 38px; border: 0; outline: 0; background: none; }
  textarea {
    margin-top: 12px;
    resize: vertical;
    border: 0;
    outline: 0;
    background: var(--md-sys-color-surface-container);
    border-radius: var(--md-sys-shape-corner-medium);
    padding: 12px;
  }
</style>
