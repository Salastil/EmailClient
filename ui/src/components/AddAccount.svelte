<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
  import { api } from '../lib/api';
  import type { Account, Discovered, OAuthProvider, Security } from '../lib/types';

  let { onclose, onadded }: { onclose: () => void; onadded: (a: Account) => void } = $props();

  const tiles = [
    { id: 'gmail', name: 'Google', hint: 'Gmail, Calendar & Contacts', color: '#ea4335', glyph: 'G', domain: 'gmail.com' },
    { id: 'outlook', name: 'Microsoft', hint: 'Outlook, Hotmail, Live', color: '#0078d4', glyph: 'O', domain: 'outlook.com' },
    { id: 'yahoo', name: 'Yahoo / AOL', hint: 'Yahoo Mail, AOL Mail', color: '#6001d2', glyph: 'Y!', domain: 'yahoo.com' },
    { id: 'fastmail', name: 'Fastmail', hint: 'Mail, calendar & contacts', color: '#2e7d32', glyph: 'F', domain: 'fastmail.com' },
    { id: 'proton', name: 'Proton Mail', hint: 'Built-in bridge, nothing else to install', color: '#6d4aff', glyph: 'P', domain: 'proton.me' },
    { id: 'cockli', name: 'cock.li', hint: 'mail.cock.li', color: '#8e24aa', glyph: 'C', domain: 'cock.li' },
    { id: 'icloud', name: 'iCloud', hint: 'Needs an app-specific password', color: '#9aa1a9', glyph: '', domain: 'icloud.com' },
    { id: 'imap', name: 'Other (IMAP)', hint: 'Settings are detected automatically', color: '#4dabf7', glyph: '@', domain: '' },
  ];
  const oauthNames: Record<OAuthProvider, string> = {
    google: 'Google', microsoft: 'Microsoft', yahoo: 'Yahoo', aol: 'AOL', fastmail: 'Fastmail',
  };

  type Step = 'pick' | 'email' | 'details' | 'waiting';
  let step = $state<Step>('pick');
  let tile = $state<(typeof tiles)[number] | null>(null);
  let email = $state('');
  let senderName = $state('');
  let password = $state('');
  let totp = $state('');
  let mailboxPassword = $state('');
  let username = $state('');
  let found = $state<Discovered | null>(null);
  let usePassword = $state(false);
  let showServers = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let authUrl = $state<string | null>(null);
  let copied = $state(false);

  const oauth = $derived(found?.oauth && !usePassword ? found.oauth : null);

  onMount(() => {
    const un = listen<string>('oauth-url', (e) => (authUrl = e.payload));
    return () => { un.then((f) => f()); };
  });

  function pick(t: (typeof tiles)[number]) {
    tile = t;
    email = '';
    step = 'email';
    error = null;
  }

  async function discover() {
    error = null;
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email.trim())) {
      error = 'Enter a full email address.';
      return;
    }
    busy = true;
    try {
      found = await api.discoverAccount(email.trim());
      username = found.username_is_email ? email.trim() : email.trim().split('@')[0];
      usePassword = false;
      step = 'details';
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function connect() {
    if (!found) return;
    error = null;
    busy = true;
    if (oauth) {
      authUrl = null;
      step = 'waiting';
    }
    try {
      const account = await api.addAccount({
        email: email.trim(),
        sender_name: senderName,
        provider: found.provider,
        username,
        password: oauth ? undefined : password,
        imap: found.imap,
        smtp: found.smtp,
        oauth,
        proton: found.proton,
        totp: totp || undefined,
        mailbox_password: mailboxPassword || undefined,
      });
      onadded(account);
    } catch (e) {
      error = String(e);
      if (step === 'waiting') step = 'details';
    } finally {
      busy = false;
    }
  }

  function cancelWaiting() {
    api.cancelOAuth();
  }

  async function copyLink() {
    if (authUrl) {
      await navigator.clipboard.writeText(authUrl);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    }
  }

  const securities: { v: Security; label: string }[] = [
    { v: 'tls', label: 'SSL/TLS' },
    { v: 'starttls', label: 'STARTTLS' },
    { v: 'none', label: 'None (localhost only)' },
  ];
</script>

<Dialog
  title={step === 'pick' ? 'Add an email account' : step === 'waiting' ? 'Finish signing in' : `Add ${tile?.name ?? ''} account`}
  {onclose}
  width={580}
>
  {#if step === 'pick'}
    <div class="grid">
      {#each tiles as t (t.id)}
        <button class="provider" onclick={() => pick(t)}>
          <span class="glyph" style:background={t.color}>{t.glyph || t.name[0]}</span>
          <span class="text"><b>{t.name}</b><small>{t.hint}</small></span>
        </button>
      {/each}
    </div>
  {:else if step === 'email'}
    <form class="form" onsubmit={(e) => { e.preventDefault(); discover(); }}>
    <button type="submit" class="implicit-submit" tabindex="-1" aria-hidden="true"></button>
      <label>Your name, as shown to people you write to<input bind:value={senderName} placeholder="Jane Doe" /></label>
      <label>Email address<input type="email" bind:value={email} placeholder={`you@${tile?.domain || 'example.com'}`} /></label>
    </form>
  {:else if step === 'details' && found}
    {#if found.source !== 'preset'}
      <p class="note"><Icon name="travel_explore" size={18} /> Settings found via {found.source === 'guess' ? 'a best guess; check them below' : found.source.toUpperCase()}.</p>
    {/if}
    {#if oauth}
      <div class="oauth">
        <Icon name="open_in_browser" size={48} />
        <p>Your web browser will open so you can sign in to <b>{oauthNames[oauth]}</b> and allow Moho Mail to access <b>{email}</b>. Your password is never entered here.</p>
        <button class="link" onclick={() => (usePassword = true)}>Use an app password instead</button>
      </div>
    {:else}
      <form class="form" onsubmit={(e) => { e.preventDefault(); connect(); }}>
    <button type="submit" class="implicit-submit" tabindex="-1" aria-hidden="true"></button>
        <label>{found.proton ? 'Proton password' : 'Password'}<input type="password" bind:value={password} /></label>
        {#if found.proton}
          <label>Two-factor code (if enabled)<input inputmode="numeric" bind:value={totp} placeholder="123456" /></label>
          <label>Mailbox password (two-password mode only)<input type="password" bind:value={mailboxPassword} /></label>
        {/if}
        {#if found.oauth}
          <button type="button" class="link" onclick={() => (usePassword = false)}>Sign in with the browser instead</button>
        {/if}
      </form>
    {/if}

    {#if !found.proton}
      <button class="servers-toggle" onclick={() => (showServers = !showServers)}>
        <Icon name={showServers ? 'expand_less' : 'expand_more'} /> Server settings
      </button>
      {#if showServers}
        <div class="servers">
          <label class="wide">Username<input bind:value={username} /></label>
          {#each [['Incoming (IMAP)', found.imap], ['Outgoing (SMTP)', found.smtp]] as [label, srv]}
            {@const s = srv as typeof found.imap}
            <div class="srv-title">{label}</div>
            <label>Server<input bind:value={s.host} /></label>
            <label class="port">Port<input type="number" bind:value={s.port} /></label>
            <label>Security
              <select bind:value={s.security}>
                {#each securities as o}<option value={o.v}>{o.label}</option>{/each}
              </select>
            </label>
          {/each}
        </div>
      {/if}
    {/if}
  {:else if step === 'waiting'}
    <div class="oauth">
      <div class="spinner"></div>
      <p>Waiting for you to finish signing in in your browser…</p>
      <small>If the browser didn't open, copy the link and paste it into your browser.</small>
    </div>
  {/if}

  {#if error}<p class="error"><Icon name="error" size={18} /> {error}</p>{/if}

  {#snippet actions()}
    {#if step === 'email'}
      <button class="tonal-btn" onclick={() => (step = 'pick')}>Back</button>
      <button class="filled-btn" disabled={busy} onclick={discover}>{busy ? 'Looking up…' : 'Continue'}</button>
    {:else if step === 'details'}
      <button class="tonal-btn" onclick={() => (step = 'email')}>Back</button>
      <button class="filled-btn" disabled={busy || (!oauth && !password)} onclick={connect}>
        {#if busy}Connecting…{:else if oauth}<Icon name="open_in_new" size={18} /> Sign in with browser{:else}Connect{/if}
      </button>
    {:else if step === 'waiting'}
      <button class="tonal-btn" disabled={!authUrl} onclick={copyLink}><Icon name="content_copy" size={18} /> {copied ? 'Copied' : 'Copy link'}</button>
      <button class="tonal-btn" onclick={cancelWaiting}>Cancel</button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .provider { display: flex; align-items: center; gap: 12px; padding: 12px; border-radius: var(--md-sys-shape-corner-medium); background: var(--md-sys-color-surface-container); text-align: left; }
  .provider:hover { background: var(--md-sys-color-surface-container-highest); }
  .glyph { width: 40px; height: 40px; flex: none; border-radius: 12px; display: grid; place-items: center; font-weight: 800; color: #fff; }
  .text { display: flex; flex-direction: column; min-width: 0; }
  small { color: var(--md-sys-color-on-surface-variant); }
  .oauth { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 8px; color: var(--md-sys-color-on-surface-variant); padding: 8px 0; }
  .oauth p { color: var(--md-sys-color-on-surface); margin: 0; }
  .form { display: flex; flex-direction: column; gap: 14px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--md-sys-color-on-surface-variant); }
  input, select {
    height: 42px; padding: 0 12px; border: 1px solid var(--md-sys-color-outline); border-radius: var(--md-sys-shape-corner-small);
    background: transparent; color: var(--md-sys-color-on-surface); outline: 0;
  }
  select { background: var(--md-sys-color-surface-container-high); }
  input:focus { border: 2px solid var(--md-sys-color-primary); }
  .note { display: flex; gap: 8px; align-items: center; font-size: 13px; color: var(--md-sys-color-on-surface-variant); margin: 0 0 12px; }
  .error { display: flex; gap: 8px; align-items: flex-start; color: #ffb4ab; font-size: 13px; margin: 12px 0 0; user-select: text; }
  .link { color: var(--md-sys-color-primary); font-size: 13px; align-self: center; }
  .link:hover { text-decoration: underline; }
  .servers-toggle { display: flex; align-items: center; gap: 4px; margin-top: 14px; color: var(--md-sys-color-on-surface-variant); }
  .servers { display: grid; grid-template-columns: 1fr 90px 150px; gap: 10px; margin-top: 8px; }
  .servers .wide { grid-column: 1 / -1; }
  .srv-title { grid-column: 1 / -1; font-weight: 600; margin-top: 6px; }
  .spinner { width: 40px; height: 40px; border-radius: 50%; border: 4px solid var(--md-sys-color-surface-container-highest); border-top-color: var(--md-sys-color-primary); animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
