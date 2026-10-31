<script lang="ts">
  // Comma-separated address field with autocomplete from past correspondents and contacts.
  import { api } from '../lib/api';
  import type { Address } from '../lib/types';

  let { value = $bindable(''), placeholder = '' }: { value: string; placeholder?: string } = $props();

  let suggestions = $state<Address[]>([]);
  let active = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const current = () => value.split(/[,;]/).pop()!.trim();

  function oninput() {
    clearTimeout(timer);
    const q = current();
    if (q.length < 2) {
      suggestions = [];
      return;
    }
    timer = setTimeout(async () => {
      suggestions = await api.completeAddress(q).catch(() => []);
      active = 0;
    }, 120);
  }

  function choose(a: Address) {
    const parts = value.split(/[,;]/);
    parts[parts.length - 1] = ' ' + (a.name ? `${a.name} <${a.email}>` : a.email);
    value = parts.join(',').replace(/^\s+/, '') + ', ';
    suggestions = [];
  }

  function onkeydown(e: KeyboardEvent) {
    if (!suggestions.length) return;
    if (e.key === 'ArrowDown') active = (active + 1) % suggestions.length;
    else if (e.key === 'ArrowUp') active = (active + suggestions.length - 1) % suggestions.length;
    else if (e.key === 'Enter' || e.key === 'Tab') choose(suggestions[active]);
    else if (e.key === 'Escape') suggestions = [];
    else return;
    e.preventDefault();
  }
</script>

<div class="wrap">
  <input bind:value {placeholder} {oninput} {onkeydown} onblur={() => setTimeout(() => (suggestions = []), 150)} />
  {#if suggestions.length}
    <div class="menu" role="listbox">
      {#each suggestions as s, i}
        <button role="option" aria-selected={i === active} class:on={i === active} onmousedown={(e) => { e.preventDefault(); choose(s); }}>
          {#if s.name}<b>{s.name}</b>{/if} <span>{s.email}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap { position: relative; flex: 1; }
  input {
    width: 100%; height: 38px; padding: 0 12px; outline: 0;
    border: 1px solid transparent; border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-surface-container);
  }
  input:focus { border-color: var(--md-sys-color-primary); }
  .menu {
    position: absolute; z-index: 5; top: 100%; left: 0; right: 0; padding: 4px;
    background: var(--md-sys-color-surface-container-highest); border-radius: var(--md-sys-shape-corner-small);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.4); display: flex; flex-direction: column;
  }
  button { text-align: left; padding: 6px 10px; border-radius: 4px; display: flex; gap: 8px; }
  button.on, button:hover { background: var(--md-sys-state-selected); }
  span { color: var(--md-sys-color-on-surface-variant); }
</style>
