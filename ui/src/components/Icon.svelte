<script lang="ts">
  let { name, size = 20, fill = false }: { name: string; size?: number; fill?: boolean } = $props();
</script>

<span
  class="material-symbols-rounded"
  aria-hidden="true"
  style:font-size="{size}px"
  style:font-variation-settings="'FILL' {fill ? 1 : 0}, 'wght' 400, 'GRAD' 0, 'opsz' {size}"
>{name}</span>

<style>
  span { line-height: 1; display: inline-block; vertical-align: middle; }
</style>
