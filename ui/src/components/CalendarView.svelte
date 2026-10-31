<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import AddCalendar from './AddCalendar.svelte';
  import EventDialog from './EventDialog.svelte';
  import Icon from './Icon.svelte';
  import { pim } from '../lib/api';
  import { addDays, allDayCovers, isoWeek, lanes, sameDay, startOfWeek } from '../lib/calendar';
  import type { Account, Instance, Source, TaskItem } from '../lib/types';

  let { accounts, newEventSignal = 0 }: { accounts: Account[]; newEventSignal?: number } = $props();

  type Mode = 'day' | 'week' | 'multiweek' | 'month';
  const today = new Date();
  let mode = $state<Mode>('week');
  let cursor = $state(new Date(today));
  let miniMonth = $state(new Date(today.getFullYear(), today.getMonth(), 1));
  let sources = $state<Source[]>([]);
  let events = $state<Instance[]>([]);
  let miniEvents = $state<Instance[]>([]);
  let tasks = $state<TaskItem[]>([]);
  let showDone = $state(false);
  let tasksOpen = $state(true);
  let taskSource = $state('local-home');
  let editing = $state<{ instance: Instance | null; start: Date } | null>(null);
  let addingCalendar = $state(false);
  let error = $state<string | null>(null);

  const HOURS = Array.from({ length: 24 }, (_, i) => i);
  const HOUR_PX = 56;

  const calendars = $derived(sources.filter((s) => s.kind === 'calendar'));
  const groups = $derived.by(() => {
    const m = new Map<string, Source[]>();
    for (const s of calendars) m.set(s.group, [...(m.get(s.group) ?? []), s]);
    return [...m.entries()];
  });
  const taskSources = $derived(calendars.filter((s) => s.todos && s.writable && s.enabled));
  const colorOf = (id: string) => sources.find((s) => s.id === id)?.color ?? '#888';

  const days = $derived.by(() => {
    if (mode === 'day') {
      const d = new Date(cursor);
      d.setHours(0, 0, 0, 0);
      return [d];
    }
    const start = startOfWeek(cursor);
    return Array.from({ length: 7 }, (_, i) => addDays(start, i));
  });

  const gridWeeks = $derived.by(() => {
    if (mode === 'month') {
      const start = startOfWeek(new Date(cursor.getFullYear(), cursor.getMonth(), 1));
      return Array.from({ length: 6 }, (_, w) => Array.from({ length: 7 }, (_, d) => addDays(start, w * 7 + d)));
    }
    const start = startOfWeek(cursor);
    return Array.from({ length: 4 }, (_, w) => Array.from({ length: 7 }, (_, d) => addDays(start, w * 7 + d)));
  });

  const range = $derived.by(() => {
    if (mode === 'day' || mode === 'week') return [days[0], addDays(days[days.length - 1], 1)] as const;
    const flat = gridWeeks.flat();
    return [flat[0], addDays(flat[flat.length - 1], 1)] as const;
  });

  const miniDays = $derived.by(() => {
    const start = startOfWeek(miniMonth);
    return Array.from({ length: 6 }, (_, w) => Array.from({ length: 7 }, (_, d) => addDays(start, w * 7 + d)));
  });

  const rangeLabel = $derived.by(() => {
    const fmt = (d: Date, o: Intl.DateTimeFormatOptions) => d.toLocaleDateString([], o);
    if (mode === 'day') return fmt(cursor, { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric' });
    if (mode === 'month') return fmt(cursor, { month: 'long', year: 'numeric' });
    const [s, e] = range;
    return `${fmt(s, { month: 'short', day: 'numeric' })} – ${fmt(addDays(e, -1), { month: 'short', day: 'numeric', year: 'numeric' })}`;
  });

  async function load() {
    try {
      sources = await pim.sources();
      const [from, to] = range;
      events = await pim.events(from, to);
      const mStart = miniDays[0][0];
      miniEvents = await pim.events(mStart, addDays(mStart, 42));
      tasks = await pim.tasks();
      if (!taskSources.some((s) => s.id === taskSource)) taskSource = taskSources[0]?.id ?? '';
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    void range;
    void miniMonth;
    load();
  });

  onMount(() => {
    const un = listen('pim-changed', () => load());
    return () => { un.then((f) => f()); };
  });

  let lastSignal = 0;
  $effect(() => {
    if (newEventSignal && newEventSignal !== lastSignal) {
      lastSignal = newEventSignal;
      newEvent();
    }
  });

  function step(dir: number) {
    const n = new Date(cursor);
    if (mode === 'day') n.setDate(n.getDate() + dir);
    else if (mode === 'month') n.setMonth(n.getMonth() + dir);
    else n.setDate(n.getDate() + 7 * dir);
    cursor = n;
  }

  function goto(d: Date) {
    cursor = new Date(d);
    miniMonth = new Date(d.getFullYear(), d.getMonth(), 1);
  }

  function newEvent(at?: Date) {
    const start = at ? new Date(at) : new Date(cursor);
    if (!at) start.setHours(Math.min(new Date().getHours() + 1, 23), 0, 0, 0);
    editing = { instance: null, start };
  }

  const allDayOn = (d: Date) => events.filter((e) => e.all_day && allDayCovers(e.start, e.end, d));
  const timedOn = (d: Date) => events.filter((e) => !e.all_day && sameDay(new Date(e.start), d));
  const anyOn = (list: Instance[], d: Date) =>
    list.some((e) => (e.all_day ? allDayCovers(e.start, e.end, d) : sameDay(new Date(e.start), d)));

  function timedStyle(e: Instance, lane: number, n: number) {
    const s = new Date(e.start);
    const top = (s.getHours() + s.getMinutes() / 60) * HOUR_PX;
    const height = Math.max(22, ((+new Date(e.end) - +s) / 3600000) * HOUR_PX - 2);
    return `top:${top}px;height:${height}px;left:calc(${(lane / n) * 100}% + 2px);width:calc(${100 / n}% - 4px);--c:${colorOf(e.source_id)}`;
  }

  function clickSlot(d: Date, hour: number) {
    const at = new Date(d);
    at.setHours(hour, 0, 0, 0);
    newEvent(at);
  }

  let grid = $state<HTMLDivElement>();
  $effect(() => {
    if (grid && (mode === 'week' || mode === 'day')) grid.scrollTop = 7.5 * HOUR_PX;
  });

  async function toggleSource(s: Source) {
    s.enabled = !s.enabled;
    await pim.updateSource(s.id, { enabled: s.enabled }).catch((e) => (error = String(e)));
    load();
  }

  async function addTask(e: SubmitEvent) {
    e.preventDefault();
    const input = (e.currentTarget as HTMLFormElement).elements.namedItem('t') as HTMLInputElement;
    if (!input.value.trim() || !taskSource) return;
    await pim.saveTask(taskSource, input.value.trim(), null).catch((err) => (error = String(err)));
    input.value = '';
  }

  async function toggleTask(t: TaskItem) {
    t.completed = !t.completed;
    await pim.setTaskDone(t.source_id, t.href, t.completed).catch((e) => (error = String(e)));
  }

  const time = (iso: string) => new Date(iso).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  const hourLabel = (h: number) => new Date(2000, 0, 1, h).toLocaleTimeString([], { hour: 'numeric' });
  const now = new Date();
</script>

<div class="calendar">
  <aside class="side">
    <button class="filled-btn new" onclick={() => newEvent()}><Icon name="add" /> New Event</button>

    <div class="mini">
      <div class="mini-head">
        <button class="icon-btn" aria-label="Previous month" onclick={() => (miniMonth = new Date(miniMonth.getFullYear(), miniMonth.getMonth() - 1, 1))}><Icon name="chevron_left" /></button>
        <span>{miniMonth.toLocaleDateString([], { month: 'long', year: 'numeric' })}</span>
        <button class="icon-btn" aria-label="Next month" onclick={() => (miniMonth = new Date(miniMonth.getFullYear(), miniMonth.getMonth() + 1, 1))}><Icon name="chevron_right" /></button>
      </div>
      <div class="mini-grid">
        <span class="cw">CW</span>
        {#each ['S', 'M', 'T', 'W', 'T', 'F', 'S'] as d}<span class="dow">{d}</span>{/each}
        {#each miniDays as week}
          <span class="cw">{isoWeek(week[1])}</span>
          {#each week as d}
            <button
              class="mday"
              class:other={d.getMonth() !== miniMonth.getMonth()}
              class:today={sameDay(d, today)}
              class:sel={sameDay(d, cursor)}
              onclick={() => goto(d)}
            >
              {d.getDate()}
              {#if anyOn(miniEvents, d)}<i></i>{/if}
            </button>
          {/each}
        {/each}
      </div>
    </div>

    <div class="cal-list">
      <div class="cal-title">Calendars <button class="icon-btn" title="Sync now" onclick={() => pim.sync()}><Icon name="sync" size={18} /></button></div>
      {#each groups as [group, list]}
        <div class="group">{group}</div>
        {#each list as s (s.id)}
          <div class="cal">
            <input type="checkbox" checked={s.enabled} style:accent-color={s.color} onchange={() => toggleSource(s)} aria-label="Show {s.name}" />
            <span class="swatch" style:background={s.color}></span>
            <span class="cal-name" title={s.name}>{s.name}</span>
            {#if !s.writable}<Icon name="lock" size={16} />{/if}
            {#if s.backend !== 'local'}
              <button class="icon-btn small" title="Remove calendar" onclick={() => pim.removeSource(s.id).then(load)}><Icon name="close" size={16} /></button>
            {/if}
          </div>
        {/each}
      {/each}
      <button class="add-cal" onclick={() => (addingCalendar = true)}><Icon name="add" size={18} /> Add calendar…</button>
    </div>
    {#if error}<p class="err" title={error}>{error}</p>{/if}
  </aside>

  <section class="main">
    <header>
      <button class="icon-btn" aria-label="Previous" onclick={() => step(-1)}><Icon name="chevron_left" /></button>
      <button class="tonal-btn" onclick={() => goto(today)}>Today</button>
      <button class="icon-btn" aria-label="Next" onclick={() => step(1)}><Icon name="chevron_right" /></button>
      <h2>{rangeLabel}</h2>
      <span class="cwk">CW {isoWeek(cursor)}</span>
      <div class="segmented" role="tablist">
        {#each [['day', 'Day'], ['week', 'Week'], ['multiweek', 'Multiweek'], ['month', 'Month']] as [m, label]}
          <button role="tab" aria-selected={mode === m} class:on={mode === m} onclick={() => (mode = m as Mode)}>{label}</button>
        {/each}
      </div>
      <button class="icon-btn" class:on={tasksOpen} title="Tasks" onclick={() => (tasksOpen = !tasksOpen)}><Icon name="task_alt" /></button>
    </header>

    {#if mode === 'day' || mode === 'week'}
      <div class="week" style:--cols={days.length}>
        <div class="week-head">
          <span></span>
          {#each days as d}
            <div class="day-name" class:today={sameDay(d, today)}>
              {d.toLocaleDateString([], { weekday: 'short' })} <b>{d.getDate()}</b>
            </div>
          {/each}
        </div>
        <div class="allday">
          <span class="gutter">all-day</span>
          {#each days as d}
            <div class="allday-cell">
              {#each allDayOn(d) as e (e.href + e.start)}
                <button class="chip" style:--c={colorOf(e.source_id)} onclick={() => (editing = { instance: e, start: d })}>
                  {e.summary}
                  {#if e.alarm_minutes !== null}<Icon name="notifications" size={14} />{/if}
                </button>
              {/each}
            </div>
          {/each}
        </div>
        <div class="grid" bind:this={grid}>
          <div class="hours">
            {#each HOURS as h}<div class="hour" style:height="{HOUR_PX}px">{h ? hourLabel(h) : ''}</div>{/each}
          </div>
          {#each days as d}
            <div class="col" class:today={sameDay(d, today)} class:weekend={d.getDay() === 0 || d.getDay() === 6}>
              {#each HOURS as h}
                <button class="slot" style:height="{HOUR_PX}px" aria-label="New event at {hourLabel(h)}" ondblclick={() => clickSlot(d, h)}></button>
              {/each}
              {#each lanes(timedOn(d)) as { ev, lane, lanes: n } (ev.href + ev.start)}
                <button class="event" class:short={+new Date(ev.end) - +new Date(ev.start) < 45 * 60000} style={timedStyle(ev, lane, n)} onclick={() => (editing = { instance: ev, start: d })}>
                  <b>{ev.summary}</b>
                  <span>{time(ev.start)} – {time(ev.end)}</span>
                  {#if ev.location}<span>{ev.location}</span>{/if}
                  {#if ev.alarm_minutes !== null}<span class="bell"><Icon name="notifications" size={14} /></span>{/if}
                </button>
              {/each}
              {#if sameDay(d, now)}
                <div class="now" style:top="{(now.getHours() + now.getMinutes() / 60) * HOUR_PX}px"></div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div class="month" style:--rows={gridWeeks.length}>
        {#each ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'] as d}<div class="mhead">{d}</div>{/each}
        {#each gridWeeks as week}
          {#each week as d}
            <div class="mcell" class:other={mode === 'month' && d.getMonth() !== cursor.getMonth()} class:today={sameDay(d, today)} role="gridcell" tabindex="-1" ondblclick={() => clickSlot(d, 9)}>
              <span class="num">{d.getDate()}</span>
              {#each [...allDayOn(d), ...timedOn(d)] as e (e.href + e.start)}
                <button class="chip" style:--c={colorOf(e.source_id)} onclick={() => (editing = { instance: e, start: d })}>
                  {e.all_day ? '' : time(e.start) + ' '}{e.summary}
                </button>
              {/each}
            </div>
          {/each}
        {/each}
      </div>
    {/if}
  </section>

  {#if tasksOpen}
    <aside class="tasks">
      <header>
        <span class="tasks-title">Tasks</span>
        <button class="icon-btn" aria-label="Close tasks" onclick={() => (tasksOpen = false)}><Icon name="close" /></button>
      </header>
      <label class="show-done"><input type="checkbox" bind:checked={showDone} /> Show completed tasks</label>
      <div class="task-list">
        {#each tasks.filter((t) => showDone || !t.completed) as t (t.source_id + t.href)}
          <div class="task" class:done={t.completed}>
            <input type="checkbox" checked={t.completed} onchange={() => toggleTask(t)} aria-label="Done" />
            <span class="swatch" style:background={colorOf(t.source_id)}></span>
            <span class="task-name">{t.summary}</span>
            {#if t.due}<small>{new Date(t.due).toLocaleDateString([], { month: 'short', day: 'numeric' })}</small>{/if}
            <button class="icon-btn small" title="Delete task" onclick={() => pim.deleteObject(t.source_id, t.href)}><Icon name="delete" size={16} /></button>
          </div>
        {:else}
          <p class="empty">No open tasks</p>
        {/each}
      </div>
      <form class="new-task" onsubmit={addTask}>
        {#if taskSources.length > 1}
          <select bind:value={taskSource}>
            {#each taskSources as s}<option value={s.id}>{s.name}</option>{/each}
          </select>
        {/if}
        <input name="t" placeholder="New task…" disabled={!taskSources.length} />
      </form>
    </aside>
  {/if}
</div>

{#if editing}
  <EventDialog {sources} instance={editing.instance} defaultStart={editing.start} onclose={() => { editing = null; load(); }} />
{/if}
{#if addingCalendar}
  <AddCalendar {accounts} onclose={() => { addingCalendar = false; load(); }} />
{/if}

<style>
  .calendar { flex: 1; display: flex; min-width: 0; background: var(--md-sys-color-surface); }
  .side { width: 280px; flex: none; display: flex; flex-direction: column; gap: 16px; padding: 16px; background: var(--md-sys-color-surface-container); overflow-y: auto; }
  .new { align-self: center; }
  .mini { background: var(--md-sys-color-surface-container-high); border-radius: var(--md-sys-shape-corner-medium); padding: 8px; }
  .mini-head { display: flex; align-items: center; justify-content: space-between; font-weight: 600; }
  .mini-grid { display: grid; grid-template-columns: 26px repeat(7, 1fr); text-align: center; font-size: 12px; row-gap: 2px; }
  .dow { color: var(--md-sys-color-on-surface-variant); font-weight: 600; padding: 4px 0; }
  .cw { color: var(--md-sys-color-primary); opacity: 0.7; font-size: 11px; display: grid; place-items: center; }
  .mday { position: relative; height: 28px; border-radius: var(--md-sys-shape-corner-small); }
  .mday:hover { background: var(--md-sys-state-hover); }
  .mday.other { color: var(--md-sys-color-on-surface-variant); opacity: 0.5; }
  .mday.today { color: var(--md-sys-color-primary); font-weight: 700; }
  .mday.sel { outline: 1px solid var(--md-sys-color-primary); background: var(--md-sys-state-selected); }
  .mday i { position: absolute; bottom: 2px; left: 50%; width: 4px; height: 4px; margin-left: -2px; border-radius: 50%; background: var(--md-sys-color-primary); }
  .cal-title { display: flex; align-items: center; justify-content: space-between; font-weight: 700; margin-bottom: 2px; }
  .group { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--md-sys-color-on-surface-variant); margin: 10px 4px 2px; }
  .cal { display: flex; align-items: center; gap: 8px; padding: 3px 4px; border-radius: var(--md-sys-shape-corner-small); }
  .cal:hover { background: var(--md-sys-state-hover); }
  .cal input { margin: 0; }
  .swatch { width: 10px; height: 10px; border-radius: 50%; flex: none; }
  .cal-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .icon-btn.small { width: 24px; height: 24px; opacity: 0; }
  .cal:hover .icon-btn.small, .task:hover .icon-btn.small { opacity: 1; }
  .add-cal { display: flex; align-items: center; gap: 6px; margin-top: 8px; color: var(--md-sys-color-on-surface-variant); }
  .add-cal:hover { color: var(--md-sys-color-on-surface); }
  .err { color: #ffb4ab; font-size: 12px; margin: 0; }

  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .main > header, .tasks > header { height: 52px; display: flex; align-items: center; gap: 8px; padding: 0 12px; border-bottom: 1px solid var(--md-sys-color-outline-variant); }
  h2 { margin: 0 0 0 8px; font-size: 16px; white-space: nowrap; }
  .cwk { font-size: 12px; color: var(--md-sys-color-on-surface-variant); margin-right: auto; }
  .segmented { display: flex; border: 1px solid var(--md-sys-color-outline); border-radius: var(--md-sys-shape-corner-full); overflow: hidden; }
  .segmented button { padding: 6px 14px; font-size: 13px; }
  .segmented button + button { border-left: 1px solid var(--md-sys-color-outline); }
  .segmented button.on { background: var(--md-sys-color-primary-container); color: var(--md-sys-color-on-primary-container); font-weight: 600; }
  .icon-btn.on { color: var(--md-sys-color-primary); }

  .week { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .week-head, .allday { display: grid; grid-template-columns: 64px repeat(var(--cols), 1fr); }
  .day-name { padding: 8px; text-align: center; border-left: 1px solid var(--md-sys-color-outline-variant); font-size: 13px; }
  .day-name.today { color: var(--md-sys-color-primary); }
  .allday { border-bottom: 1px solid var(--md-sys-color-outline); min-height: 34px; }
  .gutter { font-size: 11px; color: var(--md-sys-color-on-surface-variant); padding: 8px; text-align: right; }
  .allday-cell { border-left: 1px solid var(--md-sys-color-outline-variant); padding: 3px; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .grid { flex: 1; overflow-y: auto; display: grid; grid-template-columns: 64px repeat(var(--cols), 1fr); }
  .hour { font-size: 11px; color: var(--md-sys-color-on-surface-variant); text-align: right; padding-right: 8px; transform: translateY(-7px); }
  .col { position: relative; border-left: 1px solid var(--md-sys-color-outline-variant); min-width: 0; }
  .col.weekend { background: rgb(255 255 255 / 0.015); }
  .col.today { background: rgb(77 171 247 / 0.04); }
  .slot { display: block; width: 100%; border-top: 1px solid var(--md-sys-color-outline-variant); cursor: cell; }
  .slot:hover { background: var(--md-sys-state-hover); }
  .event {
    position: absolute; padding: 3px 6px; border-radius: var(--md-sys-shape-corner-small); text-align: left;
    background: color-mix(in srgb, var(--c) 80%, black); color: #0b0d10; font-size: 12px; overflow: hidden;
    display: flex; flex-direction: column; align-items: flex-start; z-index: 1;
  }
  .event:hover { filter: brightness(1.1); }
  .event.short { flex-direction: row; gap: 6px; white-space: nowrap; padding-top: 1px; }
  .event b { font-weight: 600; }
  .bell { position: absolute; right: 4px; top: 3px; }
  .now { position: absolute; left: 0; right: 0; border-top: 2px solid var(--md-sys-color-error); z-index: 2; pointer-events: none; }
  .now::before { content: ''; position: absolute; left: -5px; top: -6px; width: 10px; height: 10px; border-radius: 50%; background: var(--md-sys-color-error); }
  .chip {
    display: flex; align-items: center; gap: 4px; padding: 2px 6px; border-radius: 4px;
    background: color-mix(in srgb, var(--c) 80%, black); color: #0b0d10; font-size: 12px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; text-align: left; min-width: 0;
  }
  .month { flex: 1; display: grid; grid-template-columns: repeat(7, 1fr); grid-template-rows: auto repeat(var(--rows), 1fr); min-height: 0; }
  .mhead { padding: 8px; font-size: 12px; font-weight: 600; color: var(--md-sys-color-on-surface-variant); border-left: 1px solid var(--md-sys-color-outline-variant); }
  .mcell { border-left: 1px solid var(--md-sys-color-outline-variant); border-top: 1px solid var(--md-sys-color-outline-variant); padding: 4px; display: flex; flex-direction: column; gap: 2px; min-height: 0; overflow: hidden; outline: none; }
  .mcell.other { opacity: 0.45; }
  .mcell.today .num { background: var(--md-sys-color-primary); color: var(--md-sys-color-on-primary); }
  .num { align-self: flex-start; font-size: 12px; padding: 1px 6px; border-radius: var(--md-sys-shape-corner-full); }

  .tasks { width: 260px; flex: none; display: flex; flex-direction: column; background: var(--md-sys-color-surface-container); border-left: 1px solid var(--md-sys-color-outline-variant); }
  .tasks-title { flex: 1; font-weight: 700; }
  .show-done { display: flex; gap: 8px; align-items: center; padding: 10px 14px; font-size: 13px; }
  .task-list { flex: 1; overflow-y: auto; padding: 0 8px; }
  .task { display: flex; gap: 8px; align-items: center; padding: 6px; border-radius: var(--md-sys-shape-corner-small); }
  .task:hover { background: var(--md-sys-state-hover); }
  .task-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .task.done .task-name { text-decoration: line-through; color: var(--md-sys-color-on-surface-variant); }
  .task small { color: var(--md-sys-color-on-surface-variant); }
  .empty { color: var(--md-sys-color-on-surface-variant); font-size: 13px; padding: 6px; }
  .new-task { padding: 10px; display: flex; flex-direction: column; gap: 6px; }
  .new-task input, .new-task select {
    width: 100%; height: 34px; padding: 0 12px; border: 1px solid var(--md-sys-color-outline); border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-surface-container-low); outline: 0;
  }
</style>
