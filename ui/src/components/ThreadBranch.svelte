<script lang="ts">
  // Children of an expanded thread, drawn with a tree line like Thunderbird.
  import MessageCard from './MessageCard.svelte';
  import ThreadBranch from './ThreadBranch.svelte';
  import type { MessageSummary, ThreadNode } from '../lib/types';

  let {
    nodes,
    selected,
    onselect,
    onstar,
  }: {
    nodes: ThreadNode[];
    selected: string | null;
    onselect: (m: MessageSummary) => void;
    onstar: (m: MessageSummary) => void;
  } = $props();
</script>

<div class="branch">
  {#each nodes as node (node.message.id)}
    <div class="leaf">
      <MessageCard
        message={node.message}
        selected={selected === node.message.id}
        child
        onselect={() => onselect(node.message)}
        onstar={() => onstar(node.message)}
      />
      {#if node.children.length}
        <ThreadBranch nodes={node.children} {selected} {onselect} {onstar} />
      {/if}
    </div>
  {/each}
</div>

<style>
  .branch { margin-left: 22px; }
  .leaf { position: relative; }
  /* Vertical rail and elbow connecting each reply to its parent. */
  .leaf::before {
    content: '';
    position: absolute;
    left: -6px;
    top: -4px;
    bottom: 0;
    border-left: 2px dotted var(--md-sys-color-outline);
  }
  .leaf:last-child::before { bottom: auto; height: 30px; }
  .leaf::after {
    content: '';
    position: absolute;
    left: -6px;
    top: 26px;
    width: 12px;
    border-top: 2px dotted var(--md-sys-color-outline);
  }
</style>
