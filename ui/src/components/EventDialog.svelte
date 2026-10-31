<script lang="ts">
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
  import { pim } from '../lib/api';
  import type { Instance, Source } from '../lib/types';

  let {
    sources,
    instance = null,
    defaultStart,
    onclose,
  }: { sources: Source[]; instance?: Instance | null; defaultStart: Date; onclose: () => void } = $props();

  const pad = (n: number) => String(n).padStart(2, '0');
  const dateStr = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const timeStr = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  // All-day events are stored at UTC midnight; show their UTC date.
  const utcDate = (d: Date) => `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;

  const writable = $derived(sources.filter((s) => s.kind === 'calendar' && s.writable && s.events));
  // svelte-ignore state_referenced_locally
  const init = (() => {
    if (instance) {
      const s = new Date(instance.start);
      const e = new Date(instance.end);
      if (instance.all_day) {
        const last = new Date(e.getTime() - 86400000);
        return { start_date: utcDate(s), start_time: '09:00', end_date: utcDate(last), end_time: '10:00' };
      }
      return { start_date: dateStr(s), start_time: timeStr(s), end_date: dateStr(e), end_time: timeStr(e) };
    }
    const e = new Date(defaultStart.getTime() + 3600000);
    return { start_date: dateStr(defaultStart), start_time: timeStr(defaultStart), end_date: dateStr(e), end_time: timeStr(e) };
  })();

  // svelte-ignore state_referenced_locally
  let f = $state({
    source_id: instance?.source_id ?? sources.find((s) => s.kind === 'calendar' && s.writable && s.events && s.enabled)?.id ?? '',
    summary: instance?.summary ?? '',
    location: instance?.location ?? '',
    description: instance?.description ?? '',
    all_day: instance?.all_day ?? false,
    alarm: instance?.alarm_minutes ?? null as number | null,
    ...init,
  });
  let busy = $state(false);
  let titleInput = $state<HTMLInputElement>();
  $effect(() => {
    if (titleInput && !instance) titleInput.focus();
  });
  let error = $state<string | null>(null);
  const readOnly = $derived(!!instance && !writable.some((s) => s.id === instance!.source_id));

  const reminders: [number | null, string][] = [
    [null, 'No reminder'], [0, 'At start'], [5, '5 minutes before'], [10, '10 minutes before'],
    [15, '15 minutes before'], [30, '30 minutes before'], [60, '1 hour before'], [1440, '1 day before'],
  ];

  async function save() {
    error = null;
    let start: Date, end: Date;
    if (f.all_day) {
      start = new Date(`${f.start_date}T00:00:00Z`);
      end = new Date(new Date(`${f.end_date}T00:00:00Z`).getTime() + 86400000);
    } else {
      start = new Date(`${f.start_date}T${f.start_time}`);
      end = new Date(`${f.end_date}T${f.end_time}`);
    }
    if (isNaN(start.getTime()) || isNaN(end.getTime())) {
      error = 'Check the dates.';
      return;
    }
    busy = true;
    try {
      const moving = instance && instance.source_id !== f.source_id;
      await pim.saveEvent(f.source_id, moving ? null : instance?.href ?? null, moving ? null : instance?.uid ?? null, {
        summary: f.summary,
        description: f.description,
        location: f.location,
        start: start.toISOString(),
        end: end.toISOString(),
        all_day: f.all_day,
        alarm_minutes: f.alarm,
      });
      if (moving && instance) await pim.deleteObject(instance.source_id, instance.href);
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!instance) return;
    busy = true;
    try {
      await pim.deleteObject(instance.source_id, instance.href);
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title={instance ? (readOnly ? 'Event' : 'Edit event') : 'New event'} {onclose} width={560}>
  <form class="form" onsubmit={(e) => { e.preventDefault(); save(); }}>
    <button type="submit" class="implicit-submit" tabindex="-1" aria-hidden="true"></button>
    <input class="title" placeholder="Add title" bind:this={titleInput} bind:value={f.summary} disabled={readOnly} />
    {#if instance?.recurring}
      <p class="note"><Icon name="repeat" size={16} /> Repeating event — saving changes replaces the whole series with a single event.</p>
    {/if}
    <label class="row"><Icon name="calendar_month" />
      <select bind:value={f.source_id} disabled={readOnly}>
        {#each readOnly ? sources.filter((s) => s.id === instance?.source_id) : writable as s}
          <option value={s.id}>{s.name} · {s.group}</option>
        {/each}
      </select>
    </label>
    <label class="row check"><input type="checkbox" bind:checked={f.all_day} disabled={readOnly} /> All day</label>
    <div class="row times"><Icon name="schedule" />
      <input type="date" bind:value={f.start_date} disabled={readOnly} />
      {#if !f.all_day}<input type="time" bind:value={f.start_time} disabled={readOnly} />{/if}
      <span>to</span>
      <input type="date" bind:value={f.end_date} disabled={readOnly} />
      {#if !f.all_day}<input type="time" bind:value={f.end_time} disabled={readOnly} />{/if}
    </div>
    <label class="row"><Icon name="location_on" /><input placeholder="Location" bind:value={f.location} disabled={readOnly} /></label>
    <label class="row"><Icon name="notifications" />
      <select bind:value={f.alarm} disabled={readOnly}>
        {#each reminders as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    </label>
    <textarea rows="4" placeholder="Description" bind:value={f.description} disabled={readOnly}></textarea>
    {#if error}<p class="error"><Icon name="error" size={18} /> {error}</p>{/if}
  </form>
  {#snippet actions()}
    {#if instance && !readOnly}
      <button class="tonal-btn" disabled={busy} onclick={remove}><Icon name="delete" size={18} /> Delete</button>
    {/if}
    <span style="flex:1"></span>
    <button class="tonal-btn" onclick={onclose}>{readOnly ? 'Close' : 'Cancel'}</button>
    {#if !readOnly}
      <button class="filled-btn" disabled={busy || !f.source_id} onclick={save}>{busy ? 'Saving…' : 'Save'}</button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .form { display: flex; flex-direction: column; gap: 12px; }
  .title { font-size: 20px; height: 48px; border: 0; border-bottom: 2px solid var(--md-sys-color-outline); background: none; outline: 0; }
  .title:focus { border-color: var(--md-sys-color-primary); }
  .row { display: flex; align-items: center; gap: 10px; color: var(--md-sys-color-on-surface-variant); }
  .row input:not([type='checkbox']), .row select, textarea {
    flex: 1; height: 38px; padding: 0 10px; border: 1px solid var(--md-sys-color-outline); border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-surface-container); color: var(--md-sys-color-on-surface); outline: 0; color-scheme: dark;
  }
  .times input { flex: 0 1 auto; }
  .times input[type='date'] { min-width: 142px; }
  .times input[type='time'] { min-width: 112px; }
  .times { flex-wrap: wrap; }
  .check { padding-left: 34px; color: var(--md-sys-color-on-surface); }
  textarea { height: auto; padding: 10px; resize: vertical; font: inherit; }
  .note { display: flex; align-items: center; gap: 6px; margin: 0; font-size: 12px; color: var(--md-sys-color-on-surface-variant); }
  .error { display: flex; gap: 8px; color: #ffb4ab; font-size: 13px; margin: 0; }
</style>
