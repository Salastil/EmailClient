<script lang="ts">
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
  import { api, pim } from '../lib/api';
  import type { Account, CalendarAccountInput } from '../lib/types';

  let { accounts, onclose }: { accounts: Account[]; onclose: () => void } = $props();

  type Kind = 'mail' | 'microsoft' | 'nextcloud' | 'icloud' | 'fastmail' | 'caldav' | 'ics';
  const options: { kind: Kind; name: string; hint: string; icon: string }[] = [
    { kind: 'mail', name: 'Google or Fastmail account', hint: 'Use a mail account you already added', icon: 'account_circle' },
    { kind: 'microsoft', name: 'Microsoft 365 / Outlook.com', hint: 'Signs in with your browser', icon: 'window' },
    { kind: 'nextcloud', name: 'Nextcloud', hint: 'Approve in your browser; contacts too', icon: 'cloud' },
    { kind: 'icloud', name: 'iCloud', hint: 'Apple ID and app-specific password', icon: 'phone_iphone' },
    { kind: 'fastmail', name: 'Fastmail (app password)', hint: 'Calendars and contacts', icon: 'mail' },
    { kind: 'caldav', name: 'Other CalDAV / CardDAV', hint: 'Radicale, Baïkal, mailbox.org, SOGo…', icon: 'dns' },
    { kind: 'ics', name: 'Subscribe by address (iCal)', hint: 'Read-only; Google Calendar’s secret address works without signing in', icon: 'rss_feed' },
  ];

  let kind = $state<Kind | null>(null);
  let accountId = $state('');
  let server = $state('');
  let url = $state('');
  let name = $state('');
  let username = $state('');
  let password = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let done = $state<number | null>(null);

  const eligible = $derived(accounts.filter((a) => a.provider === 'gmail' || a.provider === 'fastmail'));

  async function add() {
    if (!kind) return;
    error = null;
    let input: CalendarAccountInput;
    switch (kind) {
      case 'mail': input = { kind: 'mail_account', account_id: accountId || eligible[0]?.id }; break;
      case 'microsoft': input = { kind: 'microsoft' }; break;
      case 'nextcloud': input = { kind: 'nextcloud', server }; break;
      case 'icloud': input = { kind: 'icloud', username, password }; break;
      case 'fastmail': input = { kind: 'fastmail', username, password }; break;
      case 'ics': input = { kind: 'ics', url, name }; break;
      default: input = { kind: 'caldav', url, username, password, name };
    }
    busy = true;
    try {
      done = await pim.addCalendarAccount(input);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  const browser = $derived(kind === 'microsoft' || kind === 'nextcloud');
</script>

<Dialog title={done !== null ? 'Calendars added' : 'Add calendars and contacts'} {onclose} width={560}>
  {#if done !== null}
    {#if kind === 'ics'}
      <p>Subscribed. Moho Mail checks the address for changes automatically; events can't be edited here.</p>
    {:else}
      <p>Added {done} {done === 1 ? 'calendar or address book' : 'calendars and address books'}. They will stay in sync automatically.</p>
    {/if}
  {:else if !kind}
    <div class="list">
      {#each options as o}
        <button class="option" onclick={() => (kind = o.kind)}>
          <Icon name={o.icon} size={28} />
          <span class="text"><b>{o.name}</b><small>{o.hint}</small></span>
          <Icon name="chevron_right" />
        </button>
      {/each}
    </div>
  {:else}
    <form class="form" onsubmit={(e) => { e.preventDefault(); add(); }}>
    <button type="submit" class="implicit-submit" tabindex="-1" aria-hidden="true"></button>
      {#if kind === 'mail'}
        {#if eligible.length}
          <label>Account
            <select bind:value={accountId}>
              {#each eligible as a}<option value={a.id}>{a.email}</option>{/each}
            </select>
          </label>
          <p class="hint">Uses the same sign-in as the mail account, so nothing else to enter.</p>
        {:else}
          <p class="hint">Add a Google or Fastmail mail account first (signed in with the browser).</p>
        {/if}
      {:else if kind === 'microsoft'}
        <p class="hint">Your browser will open so you can allow Moho Mail to read and edit your Outlook calendars.</p>
      {:else if kind === 'ics'}
        <label>Calendar address<input bind:value={url} placeholder="https://calendar.google.com/calendar/ical/…/basic.ics" /></label>
        <label>Name (optional)<input bind:value={name} /></label>
        <p class="hint">
          For Google Calendar: open calendar.google.com → Settings → pick the calendar → Integrate calendar → copy
          “Secret address in iCal format”. Keep it private: anyone with it can see your events.
        </p>
        <button type="button" class="link" onclick={() => api.openLink('https://calendar.google.com/calendar/r/settings')}>
          Open Google Calendar settings <Icon name="open_in_new" size={14} />
        </button>
      {:else if kind === 'nextcloud'}
        <label>Server address<input bind:value={server} placeholder="cloud.example.com" /></label>
        <p class="hint">Your browser will open for you to approve access; Nextcloud then gives Moho Mail an app password.</p>
      {:else}
        {#if kind === 'caldav'}
          <label>Server URL<input bind:value={url} placeholder="https://dav.example.com/" /></label>
          <label>Name (optional)<input bind:value={name} /></label>
        {/if}
        <label>{kind === 'icloud' ? 'Apple ID' : 'Username'}<input bind:value={username} /></label>
        <label>{kind === 'caldav' ? 'Password' : 'App password'}<input type="password" bind:value={password} /></label>
      {/if}
      {#if busy && browser}<p class="hint"><Icon name="open_in_browser" size={18} /> Waiting for you to finish in your browser…</p>{/if}
      {#if error}<p class="error"><Icon name="error" size={18} /> {error}</p>{/if}
    </form>
  {/if}
  {#snippet actions()}
    {#if done !== null}
      <button class="filled-btn" onclick={onclose}>Done</button>
    {:else if kind}
      <button class="tonal-btn" disabled={busy} onclick={() => { kind = null; error = null; }}>Back</button>
      <button class="filled-btn" disabled={busy || (kind === 'mail' && !eligible.length)} onclick={add}>
        {busy ? 'Connecting…' : browser ? 'Continue in browser' : 'Add'}
      </button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .list { display: flex; flex-direction: column; gap: 6px; }
  .option { display: flex; align-items: center; gap: 14px; padding: 12px 14px; border-radius: var(--md-sys-shape-corner-medium); background: var(--md-sys-color-surface-container); text-align: left; }
  .option:hover { background: var(--md-sys-color-surface-container-highest); }
  .text { flex: 1; display: flex; flex-direction: column; }
  small, .hint { color: var(--md-sys-color-on-surface-variant); font-size: 13px; }
  .hint { display: flex; gap: 6px; align-items: center; margin: 0; }
  .form { display: flex; flex-direction: column; gap: 12px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--md-sys-color-on-surface-variant); }
  input, select {
    height: 42px; padding: 0 12px; border: 1px solid var(--md-sys-color-outline); border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-surface-container); color: var(--md-sys-color-on-surface); outline: 0;
  }
  .link { align-self: flex-start; display: inline-flex; align-items: center; gap: 4px; color: var(--md-sys-color-primary); font-size: 13px; }
  .error { display: flex; gap: 8px; color: #ffb4ab; font-size: 13px; margin: 0; }
</style>
