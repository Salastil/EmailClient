<script lang="ts">
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';

  let { onclose }: { onclose: () => void } = $props();

  type Kind = 'oauth' | 'password' | 'proton';
  const providers: { id: string; name: string; hint: string; kind: Kind; color: string; glyph: string }[] = [
    { id: 'gmail', name: 'Google', hint: 'Gmail, Google Calendar & Contacts', kind: 'oauth', color: '#ea4335', glyph: 'G' },
    { id: 'outlook', name: 'Microsoft', hint: 'Outlook, Hotmail, Live', kind: 'oauth', color: '#0078d4', glyph: 'O' },
    { id: 'yahoo', name: 'Yahoo / AOL', hint: 'Yahoo Mail, AOL Mail', kind: 'oauth', color: '#6001d2', glyph: 'Y!' },
    { id: 'fastmail', name: 'Fastmail', hint: 'Mail, calendar & contacts', kind: 'oauth', color: '#2e7d32', glyph: 'F' },
    { id: 'proton', name: 'Proton Mail', hint: 'Built-in bridge, nothing else to install', kind: 'proton', color: '#6d4aff', glyph: 'P' },
    { id: 'cockli', name: 'cock.li', hint: 'mail.cock.li', kind: 'password', color: '#8e24aa', glyph: 'C' },
    { id: 'icloud', name: 'iCloud', hint: 'Needs an app-specific password', kind: 'password', color: '#9aa1a9', glyph: '' },
    { id: 'imap', name: 'Other (IMAP)', hint: 'Settings are detected automatically', kind: 'password', color: '#4dabf7', glyph: '@' },
  ];

  let chosen = $state<(typeof providers)[number] | null>(null);
  let email = $state('');
  let password = $state('');
</script>

<Dialog title={chosen ? `Add ${chosen.name} account` : 'Add an email account'} {onclose} width={560}>
  {#if !chosen}
    <div class="grid">
      {#each providers as p (p.id)}
        <button class="provider" onclick={() => (chosen = p)}>
          <span class="glyph" style:background={p.color}>{p.glyph || p.name[0]}</span>
          <span class="text"><b>{p.name}</b><small>{p.hint}</small></span>
        </button>
      {/each}
    </div>
  {:else if chosen.kind === 'oauth'}
    <div class="oauth">
      <Icon name="open_in_browser" size={48} />
      <p>Your web browser will open so you can sign in to <b>{chosen.name}</b> and allow Moho Mail to access your mail{chosen.id === 'gmail' || chosen.id === 'fastmail' ? ', calendar and contacts' : ''}. Your password is never entered here.</p>
    </div>
  {:else}
    <form class="form" onsubmit={(e) => e.preventDefault()}>
      <label>Email address<input type="email" bind:value={email} placeholder="you@{chosen.id === 'cockli' ? 'cock.li' : 'example.com'}" /></label>
      <label>{chosen.kind === 'proton' ? 'Proton password' : 'Password'}<input type="password" bind:value={password} /></label>
      {#if chosen.kind === 'proton'}
        <p class="note"><Icon name="info" size={18} /> Two-factor and mailbox passwords are asked for next if your account uses them.</p>
      {/if}
    </form>
  {/if}

  {#snippet actions()}
    {#if chosen}
      <button class="tonal-btn" onclick={() => (chosen = null)}>Back</button>
      <button class="filled-btn" disabled title="Account connection arrives with the sync engine (milestones 2–4)">
        {#if chosen.kind === 'oauth'}<Icon name="open_in_new" size={18} /> Sign in with browser{:else}Connect{/if}
      </button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .provider {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: var(--md-sys-shape-corner-medium);
    background: var(--md-sys-color-surface-container);
    text-align: left;
  }
  .provider:hover { background: var(--md-sys-color-surface-container-highest); }
  .glyph { width: 40px; height: 40px; flex: none; border-radius: 12px; display: grid; place-items: center; font-weight: 800; color: #fff; }
  .text { display: flex; flex-direction: column; min-width: 0; }
  small { color: var(--md-sys-color-on-surface-variant); }
  .oauth { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 8px; color: var(--md-sys-color-on-surface-variant); }
  .oauth p { color: var(--md-sys-color-on-surface); }
  .form { display: flex; flex-direction: column; gap: 14px; }
  .form label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--md-sys-color-on-surface-variant); }
  .form input {
    height: 44px;
    padding: 0 14px;
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-small);
    background: transparent;
    color: var(--md-sys-color-on-surface);
    outline: 0;
  }
  .form input:focus { border: 2px solid var(--md-sys-color-primary); }
  .note { display: flex; gap: 8px; align-items: center; font-size: 13px; color: var(--md-sys-color-on-surface-variant); margin: 0; }
</style>
