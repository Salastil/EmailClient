# Plan: Rust + Tauri email client ("Moho Mail" working name)

## Context
The repo `salastil/emailclient` is empty (LICENSE only). We are building a new desktop email client in Rust with a Tauri UI, laid out like the Moho chat client: 4 resizable columns (account rail → folders → threaded message list → reader), plus a calendar/planner like Thunderbird's, contacts, and settings. Must support Gmail, Outlook/Hotmail, cock.li, other common providers, and Protonmail. Calendars via Google, Nextcloud and other CalDAV services. Material Design 3 icons/layout. **No Chromium-based rendering engine.**

## Platform & rendering engine (decided)
- **Linux only.** Tauri v2 on **WebKitGTK** (`webkit2gtk-4.1`), the stable system engine, with no Chromium anywhere.
- No macOS or Windows targets, so there is no cross-platform code or CI for them.
- Email bodies render in a sandboxed `<iframe sandbox>` with strict CSP. HTML is sanitized in Rust (`ammonia`) and remote images are blocked by default.
- Frontend builds target the WebKitGTK version shipped by Arch (Safari 17-level features).

## Packaging (Arch)
- `packaging/arch/PKGBUILD` (pkgname `moho-mail`, license GPL-3.0-or-later to match the repo).
  - `depends`: `webkit2gtk-4.1 gtk3 libayatana-appindicator libsecret openssl dbus`
  - `makedepends`: `rust cargo go pnpm nodejs`
  - `build()`: `pnpm install --frozen-lockfile && pnpm tauri build --no-bundle`
  - `package()`: installs the binary, the `.desktop` file, hicolor icons and license.
- `packaging/arch/moho-mail-git` variant (`pkgver()` from git describe). `.SRCINFO` is generated with `makepkg --printsrcinfo`.
- `.desktop` file plus an XDG autostart entry (optional, written from Settings).
- Tauri's own bundler only adds `.deb`/AppImage as secondary outputs. The PKGBUILD is the primary package.

## Architecture
Cargo workspace:
- `crates/mail-core`: account model, sync engine, threading, SQLite cache (`rusqlite` + FTS5 search), credential storage (`keyring`).
- `crates/providers`: IMAP (`async-imap` + IDLE), SMTP (`lettre`), MIME (`mail-parser`, `mail-builder`), OAuth2 (`oauth2` crate, XOAUTH2 for Gmail/Outlook), JMAP (`jmap-client`, Fastmail). Provider autodiscovery: built-in presets (Gmail, Outlook/Hotmail, Yahoo, iCloud, cock.li `mail.cock.li`, Fastmail, Zoho, GMX, AOL, Proton), then the Thunderbird ISPDB, then `/.well-known/autoconfig`, then an MX lookup.
- Protonmail, with **the bridge built into our app** (the user installs and runs nothing else):
  - `crates/proton-bridge-sys`: Proton's open-source Bridge core (Go, GPLv3, the same license as this repo), specifically `go-proton-api` + `gluon` (its IMAP backend) + Bridge's SMTP/sync code. It is compiled with `go build -buildmode=c-archive` from a thin Go wrapper (`bridge-ffi/`) and statically linked into our Rust binary via `build.rs`. It runs in-process, so there's no sidecar process.
  - FFI surface:
    - `bridge_start(data_dir)` / `bridge_stop()`
    - `bridge_login(user, pass)` → may require 2FA, so `bridge_submit_totp(code)` and `bridge_submit_mailbox_password(pw)`
    - `bridge_logout(id)`
    - `bridge_imap_smtp_endpoint()` → returns 127.0.0.1, a random port and a per-session generated password
    - an event callback (sync progress, errors, needs-reauth)
  - Our IMAP/SMTP provider code then talks to that loopback endpoint, so Proton reuses the whole IMAP path (folders, threading, IDLE, notifications). The endpoint binds only to 127.0.0.1 with random credentials kept in memory and never exposed to the user.
  - Lifecycle: starts with the app (including when hidden in the tray) and stops on Quit. The bridge vault (session state) lives in `$XDG_DATA_HOME/moho-mail/proton/`, and its encrypted message cache in `$XDG_CACHE_HOME/moho-mail/proton/`. Secrets go to the Secret Service (libsecret) through `keyring`.
  - Proton "human verification" (CAPTCHA), when Proton requests it, opens in a small Tauri WebKitGTK window that loads Proton's verification page. The token is returned via FFI.
  - Proton's Bridge code is vendored as a git submodule pinned to a release tag, so upstream fixes can be pulled in.
- `crates/pim`: CalDAV/CardDAV client (`reqwest` + WebDAV REPORT, `icalendar`/`calcard` parsing). Google Calendar through its CalDAV endpoint with OAuth; Nextcloud, iCloud, Fastmail and generic CalDAV through URL + app password. Local "Home" calendar stored in SQLite.
- `src-tauri`: Tauri v2 app. Commands and events bridge `mail-core` and `pim` to the UI.
- `ui/`: Svelte 5 + Vite + TypeScript. Material Design 3 tokens (dark theme like Moho), Material Symbols icons (self-hosted font, Apache-2.0). Builds target Safari/WebKit 15 for WebKitGTK compatibility.

## Mockup (main mail view, dark theme)
```
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ✉ Moho Mail                         [ 🔍 Search mail, contacts, events…   Ctrl K ]       ─  ▢  ✕ │
├────┬───────────────────┬┬──────────────────────────────┬┬────────────────────────────────────────┤
│    │ Gmail ▾           ││ Inbox  ·  121 messages  ⚲ Filter ⇅││ twikit + x's per-read pricing     ↩ ↩↩ ↪ 🗑 ⋮ │
│[G]●│ jfarrington86@…   ││──────────────────────────────││────────────────────────────────────────│
│ 3  │                   ││ Contact <contact@…>   2:45 AM ││ (A) alex.p <alex.p@getf…>   10/8/26 10:51│
│[O] │ ▸ FAVORITES       ││ Important updates to our…  ☆ ││     To: salastil@…                     │
│    │   📥 Inbox     12 ││──────────────────────────────││                                        │
│[C]  │   ⭐ Starred      ││▌alex.p <alex.p@getf…>  9/4/26 ││  hey t,                                │
│ 1  │   📤 Sent         ││▌twikit + x's per-read pricing ││                                        │
│[P] │   📝 Drafts    2  ││▌⧉ 2 replies ▾             ☆ ││  if you watch the free tier they…      │
│    │   ⚠ Spam         ││ ┆                             ││  x api is $1.99 per … full 60x …       │
│[F] │   🗑 Trash        ││ ├─ alex.p        9/23/26 7:18 ││                                        │
│    │ ▸ FOLDERS         ││ │  Re: twikit + x's per-…  ☆ ││  ── sanitized HTML (sandboxed iframe) ──│
│    │   ▸ Receipts    4 ││ └─ alex.p       10/8/26 10:51 ││  Remote images blocked  [Load images]   │
│    │   ▸ Projects      ││    Re: twikit + x's per-…  ☆ ││                                        │
│    │ ▸ LABELS          ││──────────────────────────────││  📎 pricing.pdf  (212 KB)               │
│────│   ● Bills         ││ Poast Matrix <matrix@…> 10/1 ││                                        │
│ +  │   ● Family        ││ [poa.st] Validate your email ☆││                                        │
│ 📅 │                   ││──────────────────────────────││────────────────────────────────────────│
│ ⚙  │ (S) Salastil ● On ││ Poast Matrix          9/28   ││ [ Reply… quick reply box           ➤ ] │
└────┴───────────────────┴┴──────────────────────────────┴┴────────────────────────────────────────┘
 col1 64px fixed   col2 ⇔ drag   ││ = drag handles   col3 ⇔ drag            col4 fills the rest
```
- Col 1: [G] Gmail, [O] Outlook, [C] cock.li, [P] Proton, [F] Fastmail avatars. ● = selected (pill indicator on the left like Discord). Badges show unread counts. `+` opens a menu: Add account · New contact · New event. 📅 opens the calendar. ⚙ opens settings.
- Col 3: the thread root has a left accent bar. "⧉ 2 replies ▾" toggles the children, which are indented and connected by a tree line.

Calendar view (opened with 📅; replaces cols 2–4, the rail stays):
```
├────┬──────────────────────┬─────────────────────────────────────────────────────┬──────────────┤
│[G] │ [+ New Event]        │ ‹ › Today   Oct 25–31, 2026  [Day|Week|Multiwk|Month]│ Tasks      ✕ │
│[O] │  ‹ October 2026 ›    │      Sun 25  Mon 26     Tue 27  Wed 28   Thu … Sat   │ ☐ Show done  │
│[C] │  S M T W T F S       │ all  │ Lawn cut │       │         │                  │ ☐ Pay rent   │
│    │  …   mini month …    │ 8 AM │          │       │         │                  │ ☐ Call John  │
│    │ CALENDARS            │12 PM │          │       │ Nurse   │                  │              │
│    │  ○ Home (local)      │      │          │       │ visit 🔔│                  │              │
│ +  │  ● jfarrington86@…   │ 2 PM │          │       │         │                  │              │
│ 📅●│  ● Holidays in US 🔒 │      │          │       │         │                  │              │
│ ⚙  │  ● Nextcloud         │                                                     │ [+ New task] │
```

## UI layout
- **Col 1 – Account rail**: round avatar/provider icon per account with unread badge, drag to reorder. At the bottom: `+` (menu: Add account / Add contact / New event), calendar icon (opens the Calendar view), cog (Settings pane).
- **Col 2 – Folders**: server folders with special-use mapping (Inbox, Sent, Drafts, Trash, Spam, Archive), collapsible hierarchy, unread counts. Gmail labels appear as folders.
- **Col 3 – Message list**: threads built with the JWZ algorithm (Message-ID / In-Reply-To / References, Gmail `X-GM-THRID` when present). Each row shows sender, date, subject and a star. Thread roots show "⧉ N replies ▾" and expand inline to show indented children connected by a tree line, like the Thunderbird screenshot. Virtualized list, Quick Filter, sort control.
- **Col 4 – Reader**: header block (From/To/Date), sanitized body iframe, attachments, Reply / Reply all / Forward / Archive / Delete. Compose opens in an overlay or a separate window.
- All column dividers can be dragged (pointer-event splitter, min/max widths). Widths are saved in settings.
- Calendar view replaces cols 2–4: mini-month + calendar list on the left; Day/Week/Multiweek/Month views; tasks pane (VTODO).
- Settings: accounts, appearance, privacy (remote images), notifications, calendar sources.

## Sign-in via system browser (OAuth2, like Thunderbird)
- Adding a Google, Microsoft (Hotmail/Outlook/Live), Yahoo/AOL or Fastmail account opens the **user's default browser** via `xdg-open` (`tauri-plugin-opener`). The user signs in and approves our app on the provider's own page; there are no password fields in our app for these providers.
- Flow (RFC 8252 "OAuth for native apps"), in `crates/providers/src/oauth.rs` using the `oauth2` crate:
  1. Generate a PKCE verifier and challenge plus a random `state`.
  2. Bind a one-shot HTTP listener on `127.0.0.1:<random port>` (`tiny_http`/`hyper`). The redirect URI is `http://127.0.0.1:<port>/callback`, or `http://localhost` for Microsoft.
  3. Open the authorization URL in the browser. The app shows "Waiting for you to finish in your browser… [Cancel] [Copy link]".
  4. The callback page checks `state`, exchanges the code for tokens, shows "You can close this tab and return to Moho Mail", and raises our window from the tray.
  5. The refresh token is stored in the Secret Service (`keyring`). Access tokens refresh automatically. On `invalid_grant`, the account shows "Sign in again", which repeats the browser flow.
- One consent covers mail **and** calendar/contacts:
  - Google: `https://mail.google.com/`, `…/auth/calendar`, `…/auth/carddav`, `openid email`. IMAP/SMTP use XOAUTH2; CalDAV/CardDAV use a Bearer token.
  - Microsoft: `https://outlook.office.com/IMAP.AccessAsUser.All`, `…/SMTP.Send`, `offline_access`. Calendar for Microsoft accounts is via Graph `Calendars.ReadWrite` (Outlook has no CalDAV).
  - Yahoo/AOL/Fastmail: their OAuth endpoints with the same loopback flow.
- Client IDs: we register our own "Desktop app" OAuth clients for Google and Microsoft and ship them in the build. We don't reuse Thunderbird's IDs, which would violate their terms.
  - Caveat: until Google verifies the app (the restricted Gmail scope needs a security assessment), it runs in "testing" mode, limited to 100 listed test users.
  - Settings → Advanced lets a user enter their own client ID, which also helps people building from the PKGBUILD.
- Providers without OAuth (cock.li, generic IMAP, Nextcloud CalDAV) use the account wizard's server and password form. Nextcloud uses its Login Flow v2, which also opens the browser and returns an app password.

## File locations (XDG Base Directory + FHS)
Paths are resolved in `crates/mail-core/src/paths.rs` with the `etcetera` crate (XDG strategy). `$XDG_*` env vars are honored, falling back to the spec defaults. The app directory is named `moho-mail`, not Tauri's reverse-DNS identifier: we override `app_config_dir`/`app_cache_dir` usage and never use Tauri's defaults. **Nothing is written directly into `$HOME`** (no `~/.moho-mail`).

| Kind | Location (default) | Contents |
|---|---|---|
| Config | `$XDG_CONFIG_HOME/moho-mail/` (`~/.config/moho-mail/`) | `settings.toml` (appearance, column widths, tray/notification modes, per-account notify mode); `accounts.toml` (server/host/username/provider, **no secrets**); notification rules/VIP list |
| Cache | `$XDG_CACHE_HOME/moho-mail/` (`~/.cache/moho-mail/`) | `mail/<account>.sqlite` (message headers, bodies, FTS index; all re-downloadable from the server), attachment temp files, avatars/favicons, remote-image cache, WebKitGTK data and cache directory (set via `WebviewWindowBuilder::data_directory`), embedded Proton bridge's encrypted message cache |
| Data | `$XDG_DATA_HOME/moho-mail/` (`~/.local/share/moho-mail/`) | Only things that can't be re-downloaded: local "Home" calendar and tasks, local address book, unsent drafts/outbox, Proton bridge vault (encrypted session state) |
| State | `$XDG_STATE_HOME/moho-mail/` (`~/.local/state/moho-mail/`) | Logs (rotated), window geometry, last-opened folder |
| Runtime | `$XDG_RUNTIME_DIR/moho-mail/` | Single-instance lock/socket |
| Secrets | Secret Service (libsecret via `keyring`) | Passwords, OAuth refresh tokens, Proton credentials. Never stored in files |

The XDG spec puts local-only user data in `~/.local/share`. Deleting `~/.cache/moho-mail` must always be safe: the app simply re-syncs.

FHS (installed by the PKGBUILD):
- `/usr/bin/moho-mail`
- `/usr/share/applications/moho-mail.desktop`
- `/usr/share/icons/hicolor/*/apps/moho-mail.png` + `scalable/…svg`
- `/usr/share/metainfo/moho-mail.metainfo.xml`
- `/usr/share/licenses/moho-mail/LICENSE`
- `/usr/lib/moho-mail/` for any non-embedded resources
- Optional autostart writes `$XDG_CONFIG_HOME/autostart/moho-mail.desktop`.

## Frontend
Svelte 5 + TypeScript + Vite (default; the user didn't specify one).

## System tray
- Tauri v2 `tray-icon` feature (`TrayIconBuilder`) in `src-tauri/src/tray.rs`.
- **Left-click toggles the main window**: shown and focused → hide it; hidden or minimized → show, unminimize and focus.
- Right-click menu: Show/Hide · Compose · Pause notifications (1h / until tomorrow / off) · Quit.
- **Close and minimize go to the tray**: on `WindowEvent::CloseRequested`, call `api.prevent_close()` and then `window.hide()`. On minimize, hide the window as well. "Quit" in the tray menu is the only real exit. Both behaviors can be switched off in Settings.
- Linux caveat: Tauri's AppIndicator backend doesn't deliver left-click events. On Linux we use the `ksni` crate (StatusNotifierItem, whose `activate` fires on left-click) for KDE, GNOME with the AppIndicator extension, and wlroots bars, with Tauri's tray as the fallback.
- The tray icon shows an unread dot or count overlay (total from unmuted accounts).
- `tauri-plugin-single-instance`: launching the app a second time shows the existing window instead of starting another process. Optional `tauri-plugin-autostart` (start minimized to tray).

## Notifications
- `tauri-plugin-notification` (D-Bus `org.freedesktop.Notifications`). Each notification shows sender, subject and the account avatar. Clicking it opens the window on that message.
- Trigger: new UIDs found by IMAP IDLE or a periodic poll (JMAP push for Fastmail) after the first full sync. Nothing fires for backlog or initial sync. Several arrivals from one account within a few seconds are grouped into one "N new messages" notification.
- **Per-account mode** (Settings → Notifications, and right-click an account in the rail): **All** · **Important only** · **Muted**. Muted accounts also drop out of the tray unread count, and their rail icon shows a 🔕 badge.
- "Important" means any of these (each toggleable):
  - Gmail `\Important` (via `X-GM-LABELS`)
  - Sender is in Contacts or the VIP list
  - Message is flagged/starred, or the thread already has a reply from you
  - Matches a user rule (sender/domain/subject contains)
- Optional per-folder override (e.g. mute Spam and Promotions, which are muted by default). Global Do-Not-Disturb and quiet hours.
- Code: `mail-core/src/notify.rs` (decision logic, unit-tested) emits events to `src-tauri`, which shows the notification and updates the tray.

## Milestones
1. Scaffold the workspace, Tauri v2 + Svelte shell, 4-column resizable layout with mock data, Material theme/icons.
2. IMAP read path: add account (password + autodiscovery), folder list, header sync into SQLite, threading, reader with sanitization.
3. Browser-based OAuth2 sign-in (loopback + PKCE) for Google, Microsoft, Yahoo/AOL, Fastmail; SMTP send, compose, reply and forward; flags, move, delete; IDLE push; system tray (close/minimize to tray, click toggle); notifications with per-account All / Important / Muted.
4. Embedded Proton bridge (in-process Go core via FFI, login/2FA/mailbox-password UI, CAPTCHA window); JMAP (Fastmail).
5. CalDAV/CardDAV, calendar UI, tasks, contacts; `+` menu actions.
6. Search (FTS5), PKGBUILD + `.desktop`/icons, `makepkg` build test.

Note: milestone 1 also includes the `PKGBUILD` skeleton so every milestone can be built with `makepkg -si`. Milestone 4 = embedded Proton bridge (`proton-bridge-sys`) + JMAP.

## Verification
- `cargo test` (threading, MIME, autodiscovery, sanitizer unit tests); `cargo clippy`.
- Integration tests against a local Dovecot/GreenMail IMAP+SMTP container and a Radicale CalDAV server.
- `pnpm tauri dev` on Linux (WebKitGTK); drive with WebDriver (`tauri-driver`) to check the layout, column resize and thread expand/collapse.
- XDG: run with `XDG_CONFIG_HOME`/`XDG_CACHE_HOME`/`XDG_DATA_HOME`/`XDG_STATE_HOME` pointed at temp directories and confirm every file lands there and nothing is created in `$HOME`. Run `rm -rf ~/.cache/moho-mail` and confirm the app recovers by re-syncing. Unit tests for `paths.rs` env-var handling.
- `makepkg -si` in a clean Arch chroot (`extra-x86_64-build`) succeeds; `namcap` reports no errors; the app launches from the desktop entry.
- Proton: log in to a test Proton account inside the app, with no external Bridge running. Check that folders and messages appear, a sent message arrives, and `ss -ltnp` shows the IMAP/SMTP ports bound only to 127.0.0.1 by our own process.
- OAuth: add a Google account. The default browser opens on Google's consent page, the window comes back after approval, and Gmail folders plus Google Calendar both load without a second prompt. Repeat for Hotmail. Revoke access on the provider side and confirm the "Sign in again" prompt.
- Unit tests for the notification decision logic (all/important/muted × VIP/flag/Gmail-important/rules/quiet hours).
- Manual tray check on KDE and GNOME: close button hides to the tray, minimize hides, left-click toggles, Quit exits. Send mail to a muted account (no notification), an important-only account (only VIP mail notifies) and an all account.
- Manual: add a real Gmail account and a cock.li account; open a thread with replies.
