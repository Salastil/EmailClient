<script lang="ts">
  import Icon from './Icon.svelte';
  import { displayName, shortDate } from '../lib/format';
  import type { MessageSummary } from '../lib/types';

  let {
    message,
    selected,
    replies = 0,
    expanded = false,
    child = false,
    onselect,
    ontoggle,
    onstar,
    onread,
  }: {
    message: MessageSummary;
    selected: boolean;
    replies?: number;
    expanded?: boolean;
    child?: boolean;
    onselect: () => void;
    ontoggle?: () => void;
    onstar: () => void;
    onread: () => void;
  } = $props();
</script>

<div
  class="card"
  class:selected
  class:unread={message.unread}
  class:child
  class:root-thread={replies > 0}
  role="option"
  aria-selected={selected}
  tabindex="-1"
  onclick={onselect}
  onkeydown={(e) => e.key === 'Enter' && onselect()}
>
  <div class="top">
    {#if message.unread}<span class="dot" aria-label="Unread"></span>{/if}
    <span class="from" title={message.from.email}>
      {displayName(message.from)} <span class="addr">&lt;{message.from.email}&gt;</span>
    </span>
    <span class="date">{shortDate(message.date)}</span>
    <button
      class="more icon-btn"
      title={message.unread ? 'Mark as read' : 'Mark as unread'}
      onclick={(e) => { e.stopPropagation(); onread(); }}
    >
      <Icon name={message.unread ? 'drafts' : 'mark_email_unread'} size={18} />
    </button>
  </div>
  <div class="bottom">
    <span class="subject">{message.subject}</span>
    {#if message.has_attachments}<Icon name="attach_file" size={16} />{/if}
    <button
      class="star"
      class:on={message.flagged}
      title={message.flagged ? 'Unstar' : 'Star'}
      onclick={(e) => { e.stopPropagation(); onstar(); }}
    >
      <Icon name="star" size={20} fill={message.flagged} />
    </button>
  </div>
  {#if replies > 0}
    <button
      class="replies"
      aria-expanded={expanded}
      onclick={(e) => { e.stopPropagation(); ontoggle?.(); }}
    >
      <Icon name="forum" size={15} />
      {replies} {replies === 1 ? 'reply' : 'replies'}
      <Icon name={expanded ? 'expand_less' : 'expand_more'} size={16} />
    </button>
  {/if}
</div>

<style>
  .card {
    position: relative;
    margin: 4px 8px;
    padding: 8px 10px;
    border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-surface-container-low);
    border: 1px solid var(--md-sys-color-outline-variant);
    cursor: default;
    outline: none;
  }
  .card:hover { background: var(--md-sys-color-surface-container); }
  .card.selected {
    background: var(--md-sys-color-surface-container-high);
    border-color: var(--md-sys-color-outline);
  }
  .card.root-thread { box-shadow: inset 3px 0 0 var(--md-sys-color-primary); }
  .top, .bottom { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .from {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
    font-size: 13px;
  }
  .addr { font-weight: 400; color: var(--md-sys-color-on-surface-variant); }
  .date { font-size: 12px; color: var(--md-sys-color-on-surface-variant); white-space: nowrap; }
  .more { width: 24px; height: 24px; opacity: 0; }
  .card:hover .more, .card.selected .more { opacity: 1; }
  .subject {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--md-sys-color-on-surface-variant);
  }
  .unread .subject { color: var(--md-sys-color-on-surface); font-weight: 600; }
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
    background: var(--md-sys-color-primary);
  }
  .star { color: var(--md-sys-color-on-surface-variant); display: grid; place-items: center; }
  .star:hover { color: var(--md-sys-color-on-surface); }
  .star.on { color: #f9c74f; }
  .replies {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-top: 6px;
    padding: 2px 8px;
    border-radius: var(--md-sys-shape-corner-full);
    border: 1px solid var(--md-sys-color-outline);
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .replies:hover { color: var(--md-sys-color-on-surface); background: var(--md-sys-state-hover); }
</style>
