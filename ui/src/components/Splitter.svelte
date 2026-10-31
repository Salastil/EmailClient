<script lang="ts">
  // Vertical drag handle that resizes the column to its left.
  let {
    width = $bindable(),
    min,
    max,
    onrelease,
  }: { width: number; min: number; max: number; onrelease?: () => void } = $props();

  let dragging = $state(false);
  let startX = 0;
  let startWidth = 0;

  const clamp = (w: number) => Math.min(max, Math.max(min, Math.round(w)));

  function down(e: PointerEvent) {
    dragging = true;
    startX = e.clientX;
    startWidth = width;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    document.body.style.cursor = 'col-resize';
  }

  function move(e: PointerEvent) {
    if (dragging) width = clamp(startWidth + e.clientX - startX);
  }

  function up() {
    if (!dragging) return;
    dragging = false;
    document.body.style.cursor = '';
    onrelease?.();
  }

  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 64 : 16;
    if (e.key === 'ArrowLeft') width = clamp(width - step);
    else if (e.key === 'ArrowRight') width = clamp(width + step);
    else return;
    e.preventDefault();
    onrelease?.();
  }
</script>

<!-- A focusable separator is an interactive widget per WAI-ARIA. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="splitter"
  class:dragging
  role="separator"
  aria-orientation="vertical"
  aria-valuenow={width}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex="0"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onkeydown={key}
></div>

<style>
  .splitter {
    flex: 0 0 5px;
    margin: 0 -2px;
    z-index: 2;
    cursor: col-resize;
    position: relative;
    outline: none;
  }
  .splitter::after {
    content: '';
    position: absolute;
    inset: 0 2px;
    background: var(--md-sys-color-outline-variant);
    transition: background 0.12s;
  }
  .splitter:hover::after,
  .splitter:focus-visible::after,
  .splitter.dragging::after {
    background: var(--md-sys-color-primary);
    inset: 0 1px;
  }
</style>
