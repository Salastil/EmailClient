<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import ContactDialog from './ContactDialog.svelte';
  import Icon from './Icon.svelte';
  import { pim } from '../lib/api';
  import { hashColor, initials } from '../lib/format';
  import type { ContactItem, Source } from '../lib/types';

  let { oncompose }: { oncompose: (to: string) => void } = $props();

  let contacts = $state<ContactItem[]>([]);
  let sources = $state<Source[]>([]);
  let query = $state('');
  let selected = $state<ContactItem | null>(null);
  let editing = $state<ContactItem | null | 'new'>(null);

  async function load() {
    [contacts, sources] = await Promise.all([pim.contacts(), pim.sources()]);
    if (selected) selected = contacts.find((c) => c.source_id === selected!.source_id && c.href === selected!.href) ?? null;
  }

  onMount(() => {
    load();
    const un = listen('pim-changed', load);
    return () => { un.then((f) => f()); };
  });

  const visible = $derived(
    contacts.filter((c) => {
      const q = query.trim().toLowerCase();
      return !q || c.name.toLowerCase().includes(q) || c.emails.some((e) => e.includes(q)) || c.organization.toLowerCase().includes(q);
    }),
  );
  const bookName = (id: string) => sources.find((s) => s.id === id)?.name ?? '';
</script>

<div class="contacts">
  <section class="list">
    <header>
      <span class="title">Contacts</span>
      <span class="count">{contacts.length}</span>
      <span class="spacer"></span>
      <button class="icon-btn" title="New contact" onclick={() => (editing = 'new')}><Icon name="person_add" /></button>
    </header>
    <label class="search"><Icon name="search" size={18} /><input placeholder="Search contacts" bind:value={query} /></label>
    <div class="scroll">
      {#each visible as c (c.source_id + c.href)}
        <button class="row" class:on={selected === c} onclick={() => (selected = c)}>
          <span class="avatar" style:background={hashColor(c.emails[0] ?? c.name)}>{initials(c.name || c.emails[0] || '?')}</span>
          <span class="who"><b>{c.name || c.emails[0]}</b><small>{c.emails[0] ?? ''}</small></span>
        </button>
      {:else}
        <p class="empty">No contacts{query ? ' match' : ' yet'}.</p>
      {/each}
    </div>
  </section>
  <section class="detail">
    {#if selected}
      <div class="card">
        <span class="big" style:background={hashColor(selected.emails[0] ?? selected.name)}>{initials(selected.name || selected.emails[0] || '?')}</span>
        <h1>{selected.name}</h1>
        {#if selected.organization}<p class="muted">{selected.organization}</p>{/if}
        <div class="fields">
          {#each selected.emails as e}
            <div class="field"><Icon name="mail" /><span>{e}</span><button class="tonal-btn" onclick={() => oncompose(selected!.name ? `${selected!.name} <${e}>` : e)}>Write</button></div>
          {/each}
          {#each selected.phones as p}<div class="field"><Icon name="call" /><span>{p}</span></div>{/each}
          {#if selected.note}<div class="field"><Icon name="notes" /><span>{selected.note}</span></div>{/if}
          <div class="field muted"><Icon name="contacts" /><span>{bookName(selected.source_id)}</span></div>
        </div>
        <button class="tonal-btn" onclick={() => (editing = selected)}><Icon name="edit" size={18} /> Edit</button>
      </div>
    {:else}
      <div class="placeholder"><Icon name="contacts" size={56} /><p>Select a contact</p></div>
    {/if}
  </section>
</div>

{#if editing}
  <ContactDialog {sources} contact={editing === 'new' ? null : editing} onclose={() => { editing = null; load(); }} />
{/if}

<style>
  .contacts { flex: 1; display: flex; min-width: 0; }
  .list { width: 340px; flex: none; display: flex; flex-direction: column; background: var(--md-sys-color-surface-container); }
  header { height: 52px; display: flex; align-items: center; gap: 8px; padding: 0 8px 0 16px; border-bottom: 1px solid var(--md-sys-color-outline-variant); }
  .title { font-weight: 700; }
  .count { font-size: 12px; color: var(--md-sys-color-on-surface-variant); }
  .spacer { flex: 1; }
  .search { display: flex; align-items: center; gap: 6px; margin: 8px; padding: 0 10px; height: 34px; border-radius: var(--md-sys-shape-corner-full); background: var(--md-sys-color-surface-container-highest); color: var(--md-sys-color-on-surface-variant); }
  .search input { flex: 1; border: 0; outline: 0; background: none; }
  .scroll { flex: 1; overflow-y: auto; padding: 0 8px 8px; }
  .row { width: 100%; display: flex; align-items: center; gap: 12px; padding: 8px; border-radius: var(--md-sys-shape-corner-medium); text-align: left; }
  .row:hover { background: var(--md-sys-state-hover); }
  .row.on { background: var(--md-sys-state-selected); }
  .avatar, .big { width: 36px; height: 36px; flex: none; border-radius: 50%; display: grid; place-items: center; font-weight: 700; color: #fff; font-size: 13px; }
  .big { width: 80px; height: 80px; font-size: 28px; }
  .who { display: flex; flex-direction: column; min-width: 0; }
  .who small, .muted, .empty { color: var(--md-sys-color-on-surface-variant); }
  .detail { flex: 1; display: flex; justify-content: center; overflow-y: auto; background: var(--md-sys-color-surface); }
  .card { width: min(520px, 100%); padding: 48px 24px; display: flex; flex-direction: column; align-items: center; gap: 8px; user-select: text; }
  h1 { margin: 8px 0 0; font-size: 22px; }
  .fields { align-self: stretch; margin: 16px 0; display: flex; flex-direction: column; gap: 6px; }
  .field { display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-radius: var(--md-sys-shape-corner-medium); background: var(--md-sys-color-surface-container); }
  .field span { flex: 1; }
  .placeholder { align-self: center; display: grid; justify-items: center; color: var(--md-sys-color-on-surface-variant); }
</style>
