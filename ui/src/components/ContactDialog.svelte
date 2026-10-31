<script lang="ts">
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
  import { pim } from '../lib/api';
  import type { ContactItem, Source } from '../lib/types';

  let {
    sources,
    contact = null,
    preset = null,
    onclose,
  }: { sources: Source[]; contact?: ContactItem | null; preset?: { name: string; email: string } | null; onclose: () => void } = $props();

  const books = $derived(sources.filter((s) => s.kind === 'address_book' && s.writable));
  // svelte-ignore state_referenced_locally
  let f = $state({
    source_id: contact?.source_id ?? 'local-contacts',
    name: contact?.name ?? preset?.name ?? '',
    emails: (contact?.emails ?? (preset?.email ? [preset.email] : [])).join(', '),
    phones: (contact?.phones ?? []).join(', '),
    organization: contact?.organization ?? '',
    note: contact?.note ?? '',
  });
  let busy = $state(false);
  let error = $state<string | null>(null);
  const split = (s: string) => s.split(',').map((x) => x.trim()).filter(Boolean);

  async function save() {
    busy = true;
    error = null;
    try {
      const moving = contact && contact.source_id !== f.source_id;
      await pim.saveContact(f.source_id, moving ? null : contact?.href ?? null, moving ? null : contact?.uid ?? null, {
        name: f.name,
        emails: split(f.emails),
        phones: split(f.phones),
        organization: f.organization,
        note: f.note,
      });
      if (moving && contact) await pim.deleteObject(contact.source_id, contact.href);
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!contact) return;
    busy = true;
    try {
      await pim.deleteObject(contact.source_id, contact.href);
      onclose();
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }
</script>

<Dialog title={contact ? 'Edit contact' : 'New contact'} {onclose} width={500}>
  <form class="form" onsubmit={(e) => { e.preventDefault(); save(); }}>
    <button type="submit" class="implicit-submit" tabindex="-1" aria-hidden="true"></button>
    <label>Name<input bind:value={f.name} /></label>
    <label>Email addresses (comma-separated)<input bind:value={f.emails} /></label>
    <label>Phone numbers<input bind:value={f.phones} /></label>
    <label>Organization<input bind:value={f.organization} /></label>
    <label>Notes<textarea rows="3" bind:value={f.note}></textarea></label>
    <label>Address book
      <select bind:value={f.source_id}>
        {#each books as b}<option value={b.id}>{b.name} · {b.group}</option>{/each}
      </select>
    </label>
    {#if error}<p class="error"><Icon name="error" size={18} /> {error}</p>{/if}
  </form>
  {#snippet actions()}
    {#if contact}<button class="tonal-btn" disabled={busy} onclick={remove}><Icon name="delete" size={18} /> Delete</button>{/if}
    <span style="flex:1"></span>
    <button class="tonal-btn" onclick={onclose}>Cancel</button>
    <button class="filled-btn" disabled={busy} onclick={save}>Save</button>
  {/snippet}
</Dialog>

<style>
  .form { display: flex; flex-direction: column; gap: 12px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--md-sys-color-on-surface-variant); }
  input, select, textarea {
    min-height: 40px; padding: 0 12px; border: 1px solid var(--md-sys-color-outline); border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-surface-container); color: var(--md-sys-color-on-surface); outline: 0; font: inherit;
  }
  textarea { padding: 10px 12px; resize: vertical; }
  .error { display: flex; gap: 8px; color: #ffb4ab; font-size: 13px; margin: 0; }
</style>
