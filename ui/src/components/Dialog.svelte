<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    title,
    onclose,
    width = 520,
    children,
    actions,
  }: { title: string; onclose: () => void; width?: number; children: Snippet; actions?: Snippet } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={title} style:width="{width}px">
    <header>
      <h2>{title}</h2>
      <button class="icon-btn" aria-label="Close" onclick={onclose}><Icon name="close" /></button>
    </header>
    <div class="content">{@render children()}</div>
    {#if actions}<footer>{@render actions()}</footer>{/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    background: var(--md-sys-color-scrim);
  }
  .dialog {
    max-width: calc(100vw - 48px);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    border-radius: 28px;
    background: var(--md-sys-color-surface-container-high);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.5);
    user-select: text;
  }
  header { display: flex; align-items: center; padding: 20px 16px 8px 24px; }
  h2 { flex: 1; margin: 0; font-size: 22px; font-weight: 500; }
  .content { padding: 8px 24px 16px; overflow-y: auto; }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding: 8px 24px 20px; }
</style>
