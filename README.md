# Moho Mail

An email, calendar and contacts client for Linux, written in Rust with a
[Tauri](https://tauri.app) UI rendered by **WebKitGTK** (no Chromium).

The layout follows the Moho chat client: four drag-resizable columns.

| Accounts rail | Folders | Threaded message list | Reader |
|---|---|---|---|
| One icon per account with unread and status badges. At the bottom: `+` (account, contact, event), calendar, contacts and settings | Server folders and Gmail labels with unread counts | Conversations that expand inline (`2 replies ▾`) with a tree line | Sanitized HTML in a sandboxed frame; remote images blocked until you allow them |

## Features

**Mail**
- IMAP with IDLE push, a full resync every 10 minutes, and an SQLite cache with full-text search
- SMTP sending with replies threaded correctly; sent mail is filed in Sent
- Compose with attachments and address autocomplete
- Reply, reply all, forward, archive, delete, move, star and read/unread
- Attachments open in their default app or save where you choose
- Supported providers:
  - **Gmail, Outlook/Hotmail, Yahoo, AOL, Fastmail:** sign-in happens in your own browser (OAuth 2.0 with PKCE, loopback redirect), like Thunderbird
  - **cock.li, iCloud, Zoho, GMX, mailbox.org, Posteo and any IMAP server:** built-in presets, then autoconfig, the Thunderbird ISPDB and MX lookup
  - **Proton Mail:** Proton's own Bridge code runs *inside* Moho Mail, so you don't install or run anything else (see [Proton Mail](#proton-mail))

**Calendar and planner** (calendar icon in the rail)
- Day, Week, Multiweek and Month views, a mini month and a tasks pane
- Recurring events, time zones and reminders, which appear as desktop notifications
- Calendar sources:
  - CalDAV/CardDAV: **Google** (reuses the Gmail sign-in), **Nextcloud** (browser approval via Login Flow v2), **iCloud**, **Fastmail**, and any CalDAV server such as Radicale, Baïkal or SOGo
  - **Microsoft 365 / Outlook.com** through Microsoft Graph
  - A local **Home** calendar

**Contacts:** synced CardDAV address books plus a personal local one. They feed autocomplete and the "important mail" rules.

**System tray**
- Left-click shows or hides the window. Closing or minimizing sends it to the tray, and Quit is in the tray menu.
- The icon shows a dot when there is unread mail.
- The tray menu also has Compose and Pause notifications.
- Linux trays don't report left-clicks to Tauri's built-in tray, so Moho Mail uses the StatusNotifierItem protocol directly.

**Notifications:** each account can notify on **all mail**, **important only** or **muted**. "Important" is configurable:
- Gmail's Important label
- Contacts and VIP senders
- Starred mail and threads you replied to
- Custom sender, domain or subject rules

Quiet hours, Do Not Disturb and per-folder muting are also available. Clicking a notification opens the message.

## Files

Moho Mail follows the XDG Base Directory spec and never writes directly into `$HOME`.

| | Location | Contents |
|---|---|---|
| Config | `~/.config/moho-mail/` | `settings.toml`, `accounts.toml` (no secrets) |
| Cache | `~/.cache/moho-mail/` | Mail index and bodies, WebKit data, Proton message cache. Safe to delete; it re-syncs |
| Data | `~/.local/share/moho-mail/` | Local calendar and contacts (`pim.sqlite`), Proton bridge vault |
| State | `~/.local/state/moho-mail/` | Logs (`MOHO_LOG=debug` for more detail) |
| Secrets | System keyring (Secret Service) | Passwords, OAuth refresh tokens, CalDAV app passwords, Proton vault key |

Autostart, if you turn it on, writes `~/.config/autostart/moho-mail.desktop`.

## Proton Mail

The `proton-bridge` build feature compiles
[Proton Mail Bridge](https://github.com/ProtonMail/proton-bridge) (GPL-3.0, like this project) into Moho Mail as a static library. The wrapper is `crates/proton-bridge-sys/bridge-ffi/main.go`. The bridge:
- starts with the app
- keeps its encrypted vault under `~/.local/share/moho-mail`, with the vault key in your keyring
- serves IMAP/SMTP on random loopback ports that only Moho Mail uses
- has updates, autostart and telemetry turned off

Moho Mail's normal IMAP/SMTP code then handles Proton exactly like any other account. Two-factor codes and two-password mode are supported in the sign-in dialog. If Proton asks for a CAPTCHA, sign in once at account.proton.me from the same network and try again.

Building the feature needs Go and a checkout of Proton's source at the pinned tag. The PKGBUILD does this for you:

```bash
git clone --branch v3.27.1 https://github.com/ProtonMail/proton-bridge ../proton-bridge
```

```bash
PROTON_BRIDGE_SRC=$PWD/../proton-bridge pnpm tauri build --no-bundle --features proton-bridge
```

Without the feature, everything else works and the Proton option says it isn't included in the build.

## OAuth clients

Browser sign-in needs an OAuth client registered with each provider. Pass them at build time:

```bash
export MOHO_GOOGLE_CLIENT_ID=... MOHO_GOOGLE_CLIENT_SECRET=... MOHO_MICROSOFT_CLIENT_ID=...
```

Users can also enter their own in **Settings → Advanced**. For Google, create a "Desktop app" client. Until Google verifies the app (the full Gmail scope needs a security assessment), a Google project in testing mode is limited to 100 listed test users. For Microsoft, register a public client with the redirect URI `http://localhost`.

## Building

Requirements:
- Rust (stable), Node.js 20+ and pnpm
- WebKitGTK 4.1 development files
- Go, only for the Proton bridge

```bash
sudo pacman -S --needed webkit2gtk-4.1 gtk3 libsecret base-devel rust nodejs pnpm go
```

```bash
pnpm install
```

```bash
pnpm tauri dev
```

The release binary is `target/release/moho-mail`:

```bash
pnpm tauri build --no-bundle
```

### Tests

Unit tests, lints and type checks:

```bash
cargo test --workspace && cargo clippy --workspace --all-targets && pnpm check
```

Live tests run against throwaway Dovecot (IMAP), an SMTP sink and Radicale (CalDAV/CardDAV). These need `dovecot-imapd`, `python-aiosmtpd` and `radicale`:

```bash
eval "$(scripts/test-servers.sh start)"
```

```bash
cargo test --workspace
```

```bash
scripts/test-servers.sh stop
```

### Arch Linux package

`packaging/arch/PKGBUILD` builds a tagged release and `packaging/arch-git/PKGBUILD` builds the latest commit. Both include the Proton bridge.

```bash
cd packaging/arch-git && makepkg -si
```

The tray icon needs a StatusNotifierItem host. KDE Plasma and most wlroots bars (waybar and others) provide one. On GNOME, install the *AppIndicator and KStatusNotifierItem Support* extension. Without a tray, closing the window quits the app.

## Repository layout

```
crates/mail-core/          model, threading, notification policy, sanitizer, store, XDG paths, settings, secrets
crates/providers/          IMAP, SMTP, OAuth 2.0, autodiscovery
crates/pim/                iCalendar, vCard, CalDAV/CardDAV, Microsoft Graph, Nextcloud login, PIM store
crates/proton-bridge-sys/  in-process Proton Mail Bridge (Go wrapper + Rust bindings)
src-tauri/                 app: sync engine, PIM sync and reminders, tray, notifications, commands
ui/                        Svelte 5 + TypeScript frontend (Material Design 3, Material Symbols)
packaging/                 .desktop, metainfo, icon, PKGBUILDs
scripts/                   test servers
docs/PLAN.md               design plan
```

## License

GPL-3.0-or-later
