<script lang="ts">
  import Icon from './Icon.svelte';
  import {
    addDays, calendars as initialCalendars, demoEvents, demoTasks, isoWeek, sameDay, startOfWeek,
    type CalEvent,
  } from '../lib/calendar';

  let { onnewevent }: { onnewevent: () => void } = $props();

  type Mode = 'day' | 'week' | 'multiweek' | 'month';
  const today = new Date();
  let mode = $state<Mode>('week');
  let cursor = $state(new Date(today));
  let miniMonth = $state(new Date(today.getFullYear(), today.getMonth(), 1));
  let sources = $state(initialCalendars.map((c) => ({ ...c })));
  let tasks = $state(demoTasks.map((t) => ({ ...t })));
  let showDone = $state(false);
  let tasksOpen = $state(true);
  const events = demoEvents(today);

  const HOURS = Array.from({ length: 24 }, (_, i) => i);
  const HOUR_PX = 56;

  const colorOf = (id: string) => sources.find((s) => s.id === id)?.color ?? '#888';
  const visibleEvents = $derived(events.filter((e) => sources.find((s) => s.id === e.calendar)?.enabled));

  const days = $derived.by(() => {
    if (mode === 'day') return [new Date(cursor)];
    const start = startOfWeek(cursor);
    return Array.from({ length: 7 }, (_, i) => addDays(start, i));
  });

  const gridWeeks = $derived.by(() => {
    if (mode === 'month') {
      const first = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
      const start = startOfWeek(first);
      return Array.from({ length: 6 }, (_, w) => Array.from({ length: 7 }, (_, d) => addDays(start, w * 7 + d)));
    }
    const start = startOfWeek(cursor);
    return Array.from({ length: 4 }, (_, w) => Array.from({ length: 7 }, (_, d) => addDays(start, w * 7 + d)));
  });

  const miniDays = $derived.by(() => {
    const start = startOfWeek(miniMonth);
    return Array.from({ length: 6 }, (_, w) => Array.from({ length: 7 }, (_, d) => addDays(start, w * 7 + d)));
  });

  const rangeLabel = $derived.by(() => {
    const fmt = (d: Date, o: Intl.DateTimeFormatOptions) => d.toLocaleDateString([], o);
    if (mode === 'day') return fmt(cursor, { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric' });
    if (mode === 'month') return fmt(cursor, { month: 'long', year: 'numeric' });
    const s = days[0];
    const e = mode === 'week' ? days[6] : addDays(s, 27);
    return `${fmt(s, { month: 'short', day: 'numeric' })} – ${fmt(e, { month: 'short', day: 'numeric', year: 'numeric' })}`;
  });

  function step(dir: number) {
    const n = new Date(cursor);
    if (mode === 'day') n.setDate(n.getDate() + dir);
    else if (mode === 'week') n.setDate(n.getDate() + 7 * dir);
    else if (mode === 'multiweek') n.setDate(n.getDate() + 7 * dir);
    else n.setMonth(n.getMonth() + dir);
    cursor = n;
  }

  function goto(d: Date) {
    cursor = new Date(d);
    miniMonth = new Date(d.getFullYear(), d.getMonth(), 1);
  }

  const eventsOn = (d: Date, allDay?: boolean) =>
    visibleEvents.filter(
      (e) => (allDay === undefined || e.allDay === allDay) && (sameDay(e.start, d) || (e.allDay && e.start <= d && d < e.end)),
    );

  function timedStyle(e: CalEvent) {
    const top = (e.start.getHours() + e.start.getMinutes() / 60) * HOUR_PX;
    const height = Math.max(22, ((e.end.getTime() - e.start.getTime()) / 3600000) * HOUR_PX - 2);
    return `top:${top}px;height:${height}px;--c:${colorOf(e.calendar)}`;
  }

  let grid = $state<HTMLDivElement>();
  $effect(() => {
    if (grid && (mode === 'week' || mode === 'day')) grid.scrollTop = 7.5 * HOUR_PX;
  });

  const time = (d: Date) => d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  const hourLabel = (h: number) => new Date(2000, 0, 1, h).toLocaleTimeString([], { hour: 'numeric' });
</script>

<div class="calendar">
  <aside class="side">
    <button class="filled-btn new" onclick={onnewevent}><Icon name="add" /> New Event</button>

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
              {#if visibleEvents.some((e) => sameDay(e.start, d))}<i></i>{/if}
            </button>
          {/each}
        {/each}
      </div>
    </div>

    <div class="cal-list">
      <div class="cal-title">Calendars</div>
      {#each sources as s (s.id)}
        <label class="cal">
          <input type="checkbox" bind:checked={s.enabled} style:accent-color={s.color} />
          <span class="swatch" style:background={s.color}></span>
          <span class="cal-name">{s.name}</span>
          {#if s.readonly}<Icon name="lock" size={16} />{/if}
        </label>
      {/each}
      <button class="add-cal"><Icon name="add" size={18} /> Add calendar…</button>
    </div>
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
              {#each eventsOn(d, true) as e}
                <div class="chip" style:--c={colorOf(e.calendar)}>
                  {e.title}
                  {#if e.reminder}<Icon name="notifications" size={14} />{/if}
                </div>
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
              {#each HOURS as _}<div class="slot" style:height="{HOUR_PX}px"></div>{/each}
              {#each eventsOn(d, false) as e}
                <div class="event" style={timedStyle(e)}>
                  <b>{e.title}</b>
                  <span>{time(e.start)} – {time(e.end)}</span>
                  {#if e.reminder}<span class="bell"><Icon name="notifications" size={14} /></span>{/if}
                </div>
              {/each}
              {#if sameDay(d, today)}
                <div class="now" style:top="{(today.getHours() + today.getMinutes() / 60) * HOUR_PX}px"></div>
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
            <div class="mcell" class:other={mode === 'month' && d.getMonth() !== cursor.getMonth()} class:today={sameDay(d, today)}>
              <span class="num">{d.getDate()}</span>
              {#each eventsOn(d) as e}
                <div class="chip" style:--c={colorOf(e.calendar)}>{e.allDay ? '' : time(e.start) + ' '}{e.title}</div>
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
        {#each tasks.filter((t) => showDone || !t.done) as t (t.id)}
          <label class="task" class:done={t.done}>
            <input type="checkbox" bind:checked={t.done} />
            {t.title}
          </label>
        {/each}
      </div>
      <form
        class="new-task"
        onsubmit={(e) => {
          e.preventDefault();
          const input = (e.currentTarget as HTMLFormElement).elements.namedItem('t') as HTMLInputElement;
          if (input.value.trim()) tasks.push({ id: crypto.randomUUID(), title: input.value.trim(), done: false });
          input.value = '';
        }}
      >
        <input name="t" placeholder="New task in Home…" />
      </form>
    </aside>
  {/if}
</div>

<style>
  .calendar { flex: 1; display: flex; min-width: 0; background: var(--md-sys-color-surface); }
  .side {
    width: 280px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
    background: var(--md-sys-color-surface-container);
    overflow-y: auto;
  }
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
  .cal-title { font-weight: 700; margin-bottom: 6px; }
  .cal { display: flex; align-items: center; gap: 8px; padding: 5px 4px; border-radius: var(--md-sys-shape-corner-small); }
  .cal:hover { background: var(--md-sys-state-hover); }
  .cal input { margin: 0; }
  .swatch { width: 10px; height: 10px; border-radius: 50%; flex: none; }
  .cal-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .add-cal { display: flex; align-items: center; gap: 6px; margin-top: 6px; color: var(--md-sys-color-on-surface-variant); }
  .add-cal:hover { color: var(--md-sys-color-on-surface); }

  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .main > header, .tasks > header {
    height: 52px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }
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
  .allday-cell { border-left: 1px solid var(--md-sys-color-outline-variant); padding: 3px; display: flex; flex-direction: column; gap: 2px; }
  .grid { flex: 1; overflow-y: auto; display: grid; grid-template-columns: 64px repeat(var(--cols), 1fr); }
  .hour { font-size: 11px; color: var(--md-sys-color-on-surface-variant); text-align: right; padding-right: 8px; transform: translateY(-7px); }
  .col { position: relative; border-left: 1px solid var(--md-sys-color-outline-variant); }
  .col.weekend { background: rgb(255 255 255 / 0.015); }
  .col.today { background: rgb(77 171 247 / 0.04); }
  .slot { border-top: 1px solid var(--md-sys-color-outline-variant); }
  .event {
    position: absolute;
    left: 3px;
    right: 3px;
    padding: 3px 6px;
    border-radius: var(--md-sys-shape-corner-small);
    background: color-mix(in srgb, var(--c) 80%, black);
    color: #0b0d10;
    font-size: 12px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .event b { font-weight: 600; }
  .bell { position: absolute; right: 4px; top: 3px; }
  .now { position: absolute; left: 0; right: 0; border-top: 2px solid var(--md-sys-color-error); }
  .now::before { content: ''; position: absolute; left: -5px; top: -6px; width: 10px; height: 10px; border-radius: 50%; background: var(--md-sys-color-error); }
  .chip {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--c) 80%, black);
    color: #0b0d10;
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .month { flex: 1; display: grid; grid-template-columns: repeat(7, 1fr); grid-template-rows: auto repeat(var(--rows), 1fr); min-height: 0; }
  .mhead { padding: 8px; font-size: 12px; font-weight: 600; color: var(--md-sys-color-on-surface-variant); border-left: 1px solid var(--md-sys-color-outline-variant); }
  .mcell { border-left: 1px solid var(--md-sys-color-outline-variant); border-top: 1px solid var(--md-sys-color-outline-variant); padding: 4px; display: flex; flex-direction: column; gap: 2px; min-height: 0; overflow: hidden; }
  .mcell.other { opacity: 0.45; }
  .mcell.today .num { background: var(--md-sys-color-primary); color: var(--md-sys-color-on-primary); }
  .num { align-self: flex-start; font-size: 12px; padding: 1px 6px; border-radius: var(--md-sys-shape-corner-full); }

  .tasks { width: 260px; flex: none; display: flex; flex-direction: column; background: var(--md-sys-color-surface-container); border-left: 1px solid var(--md-sys-color-outline-variant); }
  .tasks-title { flex: 1; font-weight: 700; }
  .show-done { display: flex; gap: 8px; align-items: center; padding: 10px 14px; font-size: 13px; }
  .task-list { flex: 1; overflow-y: auto; padding: 0 8px; }
  .task { display: flex; gap: 8px; align-items: center; padding: 6px; border-radius: var(--md-sys-shape-corner-small); }
  .task:hover { background: var(--md-sys-state-hover); }
  .task.done { text-decoration: line-through; color: var(--md-sys-color-on-surface-variant); }
  .new-task { padding: 10px; }
  .new-task input {
    width: 100%;
    height: 34px;
    padding: 0 12px;
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-surface-container-low);
    outline: 0;
  }
</style>
