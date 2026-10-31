# Moho Mail

An email and calendar client for Linux, written in Rust with a [Tauri](https://tauri.app)
UI rendered by **WebKitGTK**. It does not use Chromium.

The layout follows the Moho chat client and has four resizable columns:

| Accounts rail | Folders | Threaded message list | Reader |
|---|---|---|---|
| One icon per account with unread badges; `+`, calendar and settings at the bottom | Server folders and labels with unread counts | Conversations that expand inline (`2 replies ▾`) with a tree line | Sanitized HTML in a sandboxed frame; remote images blocked by default |

The calendar (rail → calendar icon) has a Thunderbird-style layout: mini-month,
calendar list, Day/Week/Multiweek/Month views and a Tasks pane.

## Status

Milestone 1 is done: the app shell, every UI surface, and the core logic. Mail data
is **demo data** from `crates/mail-core/src/mock.rs` until the sync engine lands.

- [x] 4-column layout, drag-resizable columns (widths persisted), Material Design 3 dark theme, Material Symbols icons
- [x] Threading (In-Reply-To / References, Gmail thread id, subject fallback, cycle-safe)
- [x] HTML sanitizing (`ammonia`) + sandboxed iframe with its own CSP; links open in the default browser
- [x] Calendar views, tasks pane, settings pane
- [x] System tray (StatusNotifierItem): left-click toggles the window, close/minimize to tray, menu with Compose / Pause notifications / Quit
- [x] Notification policy per account: **All / Important only / Muted**, plus VIPs, rules, Gmail Important, starred, contacts, quiet hours, DND and muted folders
- [x] XDG Base Directory storage, single-instance, PKGBUILD
- [ ] IMAP sync, SQLite cache, autodiscovery (milestone 2)
- [ ] Browser-based OAuth sign-in (Google, Microsoft, Yahoo/AOL, Fastmail), SMTP, IDLE (milestone 3)
- [ ] Built-in Proton Mail bridge, in-process; nothing else to install (milestone 4)
- [ ] CalDAV / CardDAV (Google, Nextcloud, iCloud, Fastmail), Microsoft Graph calendars (milestone 5)
- [ ] Full-text search (milestone 6)

## Files

Moho Mail follows the XDG Base Directory spec and never writes directly into `$HOME`.

| | Location | Contents |
|---|---|---|
| Config | `~/.config/moho-mail/` | `settings.toml` (layout, tray, notifications), `accounts.toml` |
| Cache | `~/.cache/moho-mail/` | Mail cache, WebKit data. Safe to delete; it re-syncs |
| Data | `~/.local/share/moho-mail/` | Local calendar, drafts, Proton bridge vault |
| State | `~/.local/state/moho-mail/` | Logs, window state |
| Secrets | System keyring (Secret Service) | Passwords and OAuth tokens |

## Building

Requirements: Rust (stable), Node.js 20+, pnpm, and the WebKitGTK 4.1 development files.

```bash
sudo pacman -S --needed webkit2gtk-4.1 gtk3 libsecret base-devel rust nodejs pnpm
```

```bash
pnpm install
```

```bash
pnpm tauri dev
```

Release binary (`target/release/moho-mail`):

```bash
pnpm tauri build --no-bundle
```

Tests and checks:

```bash
cargo test --workspace && cargo clippy --workspace --all-targets && pnpm check
```

### Arch Linux package

`packaging/arch/PKGBUILD` builds a tagged release. `packaging/arch-git/PKGBUILD` builds the latest commit.

```bash
cd packaging/arch-git && makepkg -si
```

The tray icon needs a StatusNotifierItem host. KDE Plasma, most wlroots bars
(waybar and others) and GNOME with the *AppIndicator and KStatusNotifierItem
Support* extension all provide one. Without a tray, closing the window quits the app.

## Layout of the repository

```
crates/mail-core/   model, threading, notification policy, sanitizer, XDG paths, settings
src-tauri/          Tauri app: window, tray (ksni), commands
ui/                 Svelte 5 + TypeScript frontend
packaging/          .desktop, metainfo, icon, PKGBUILDs
```

## License

GPL-3.0-or-later
