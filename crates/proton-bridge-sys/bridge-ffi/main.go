// Moho Mail's in-process Proton Mail Bridge.
//
// This file is copied into a checkout of github.com/ProtonMail/proton-bridge
// (cmd/moho-ffi) at build time, because Go only lets code inside that module
// import its internal packages. It is built with -buildmode=c-archive and
// linked statically into Moho Mail, so no separate program runs.
//
// Every exported function returns a JSON document that the caller must free
// with moho_bridge_free.
package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"net/http/cookiejar"
	"strings"
	"sync"
	"unsafe"

	"github.com/Masterminds/semver/v3"
	"github.com/ProtonMail/gluon/imap"
	"github.com/ProtonMail/gluon/reporter"
	"github.com/ProtonMail/go-proton-api"
	"github.com/ProtonMail/gopenpgp/v2/crypto"
	"github.com/ProtonMail/proton-bridge/v3/internal/bridge"
	"github.com/ProtonMail/proton-bridge/v3/internal/constants"
	"github.com/ProtonMail/proton-bridge/v3/internal/cookies"
	"github.com/ProtonMail/proton-bridge/v3/internal/crash"
	"github.com/ProtonMail/proton-bridge/v3/internal/dialer"
	"github.com/ProtonMail/proton-bridge/v3/internal/events"
	"github.com/ProtonMail/proton-bridge/v3/internal/hv"
	"github.com/ProtonMail/proton-bridge/v3/internal/locations"
	"github.com/ProtonMail/proton-bridge/v3/internal/services/observability"
	"github.com/ProtonMail/proton-bridge/v3/internal/updater"
	"github.com/ProtonMail/proton-bridge/v3/internal/useragent"
	"github.com/ProtonMail/proton-bridge/v3/internal/vault"
	"github.com/ProtonMail/proton-bridge/v3/pkg/keychain"
	"github.com/sirupsen/logrus"
)

// dirs places Bridge's files under Moho Mail's XDG directories.
type dirs struct{ config, data, cache string }

func (d dirs) UserConfig() string { return d.config }
func (d dirs) UserData() string   { return d.data }
func (d dirs) UserCache() string  { return d.cache }

// noopReporter keeps crash and telemetry reports local: Moho Mail does not
// send anything to Proton beyond the API calls needed to sync mail.
type noopReporter struct{}

func (noopReporter) ReportException(any) error                                           { return nil }
func (noopReporter) ReportMessage(string) error                                          { return nil }
func (noopReporter) ReportMessageWithContext(string, reporter.Context) error             { return nil }
func (noopReporter) ReportWarningWithContext(string, reporter.Context) error             { return nil }
func (noopReporter) ReportMessageWithContextAndTags(string, reporter.Context, reporter.Tags) error {
	return nil
}
func (noopReporter) ReportExceptionWithContext(any, reporter.Context) error { return nil }

// Moho Mail handles its own autostart and updates (the system package manager).
type noopAutostart struct{}

func (noopAutostart) Enable() error   { return nil }
func (noopAutostart) Disable() error  { return nil }
func (noopAutostart) IsEnabled() bool { return false }

type noopUpdater struct{}

func (noopUpdater) RemoveOldUpdates() error { return nil }
func (noopUpdater) GetVersionInfo(context.Context, updater.Downloader) (updater.VersionInfo, error) {
	return updater.VersionInfo{}, nil
}
func (noopUpdater) InstallUpdate(context.Context, updater.Downloader, updater.Release, bool) error {
	return nil
}

type state struct {
	bridge  *bridge.Bridge
	vault   *vault.Vault
	jar     *cookies.Jar
	events  []map[string]any
	pending map[string]pendingLogin
}

// A login waiting for a second factor or the mailbox password.
type pendingLogin struct {
	client   *proton.Client
	auth     proton.Auth
	password []byte
}

var (
	mu sync.Mutex
	st *state
)

func reply(v any) *C.char {
	b, err := json.Marshal(v)
	if err != nil {
		b = []byte(`{"error":"internal: cannot encode reply"}`)
	}
	return C.CString(string(b))
}

func fail(err error) *C.char {
	return reply(map[string]any{"error": err.Error()})
}

func freePort() (int, error) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, err
	}
	defer l.Close()
	return l.Addr().(*net.TCPAddr).Port, nil
}

//export moho_bridge_free
func moho_bridge_free(p *C.char) {
	C.free(unsafe.Pointer(p))
}

// moho_bridge_start opens the vault (encrypted with vaultKey, base64) and starts
// Bridge's IMAP/SMTP servers on free loopback ports.
//
//export moho_bridge_start
func moho_bridge_start(configDir, dataDir, cacheDir, vaultKeyB64, version *C.char) *C.char {
	mu.Lock()
	defer mu.Unlock()
	if st != nil {
		return reply(status())
	}
	logrus.SetLevel(logrus.WarnLevel)

	key, err := base64.StdEncoding.DecodeString(C.GoString(vaultKeyB64))
	if err != nil || len(key) < 32 {
		return fail(errors.New("invalid vault key"))
	}
	ver, err := semver.NewVersion(C.GoString(version))
	if err != nil {
		return fail(fmt.Errorf("invalid bridge version: %w", err))
	}
	constants.Version = ver.Original()
	constants.FullAppName = "Proton Mail Bridge"

	locs := locations.New(dirs{C.GoString(configDir), C.GoString(dataDir), C.GoString(cacheDir)}, "bridge")
	crashHandler := crash.NewHandler()

	vaultDir, err := locs.ProvideSettingsPath()
	if err != nil {
		return fail(err)
	}
	gluonDir, err := locs.ProvideGluonCachePath()
	if err != nil {
		return fail(err)
	}
	v, corrupt, err := vault.New(vaultDir, gluonDir, key, crashHandler)
	if err != nil {
		return fail(fmt.Errorf("cannot open the Proton vault: %w", err))
	}
	if corrupt != nil {
		logrus.WithError(corrupt).Warn("Proton vault was corrupt and has been reset")
	}
	_ = v.SetAutoUpdate(false)
	_ = v.SetTelemetryDisabled(true)
	_ = v.SetAutostart(false)
	// Plain IMAP/SMTP on loopback only; STARTTLS is still offered.
	_ = v.SetIMAPSSL(false)
	_ = v.SetSMTPSSL(false)
	if p, err := freePort(); err == nil {
		_ = v.SetIMAPPort(p)
	}
	if p, err := freePort(); err == nil {
		_ = v.SetSMTPPort(p)
	}

	baseJar, _ := cookiejar.New(nil)
	jar, err := cookies.NewCookieJar(baseJar, v)
	if err != nil {
		_ = v.Close()
		return fail(err)
	}

	identifier := useragent.New()
	pinning := dialer.NewPinningTLSDialer(
		dialer.NewBasicTLSDialer(constants.APIHost),
		dialer.NewTLSReporter(constants.APIHost, constants.AppVersion(ver.Original()), identifier, dialer.TrustedAPIPins),
		dialer.NewTLSPinChecker(dialer.TrustedAPIPins),
	)
	proxy := dialer.NewProxyTLSDialer(pinning, constants.APIHost, crashHandler)

	var obs *observability.Service
	_ = observability.WithObservability(locs, func(s *observability.Service) error { obs = s; return nil })

	// Moho Mail supplies the vault key itself, so Bridge never needs to probe
	// or write the system keyring; an in-memory keychain list satisfies its API.
	b, eventCh, err := bridge.New(
		locs, v, noopAutostart{}, noopUpdater{}, ver, keychain.NewTestKeychainsList(), obs,
		constants.APIHost, jar, identifier, pinning, dialer.CreateTransportWithDialer(proxy), proxy,
		crashHandler, noopReporter{}, imap.DefaultEpochUIDValidityGenerator(), nil,
		false, false, false,
	)
	if err != nil {
		_ = v.Close()
		return fail(fmt.Errorf("cannot start the Proton bridge: %w", err))
	}
	st = &state{bridge: b, vault: v, jar: jar, pending: map[string]pendingLogin{}}
	go collect(eventCh)
	return reply(status())
}

// collect keeps events the app cares about until moho_bridge_events drains them.
func collect(ch <-chan events.Event) {
	for ev := range ch {
		var kind string
		var user string
		switch e := ev.(type) {
		case events.UserDeauth:
			kind, user = "deauth", e.UserID
		case events.UserBadEvent:
			kind, user = "bad_event", e.UserID
		case events.ConnStatusDown:
			kind = "offline"
		case events.ConnStatusUp:
			kind = "online"
		case events.SyncFinished:
			kind, user = "synced", e.UserID
		default:
			continue
		}
		mu.Lock()
		if st != nil && len(st.events) < 256 {
			st.events = append(st.events, map[string]any{"kind": kind, "user_id": user})
		}
		mu.Unlock()
	}
}

func status() map[string]any {
	return map[string]any{
		"running":   st != nil,
		"imap_port": st.bridge.GetIMAPPort(),
		"smtp_port": st.bridge.GetSMTPPort(),
	}
}

//export moho_bridge_events
func moho_bridge_events() *C.char {
	mu.Lock()
	defer mu.Unlock()
	if st == nil {
		return reply([]any{})
	}
	evs := st.events
	st.events = nil
	if evs == nil {
		evs = []map[string]any{}
	}
	return reply(evs)
}

func userFor(email string) (bridge.UserInfo, bool) {
	email = strings.ToLower(strings.TrimSpace(email))
	for _, id := range st.bridge.GetUserIDs() {
		info, err := st.bridge.GetUserInfo(id)
		if err != nil {
			continue
		}
		if strings.EqualFold(info.Username, email) {
			return info, true
		}
		for _, a := range info.Addresses {
			if strings.EqualFold(a, email) {
				return info, true
			}
		}
	}
	return bridge.UserInfo{}, false
}

// moho_bridge_login signs in. If Proton needs more (TOTP, mailbox password or
// human verification), the reply says so and the call is repeated with it.
//
//export moho_bridge_login
func moho_bridge_login(email, password, totp, mailboxPassword, hvToken *C.char) *C.char {
	mu.Lock()
	defer mu.Unlock()
	if st == nil {
		return fail(errors.New("the Proton bridge is not running"))
	}
	ctx := context.Background()
	user := strings.ToLower(strings.TrimSpace(C.GoString(email)))

	if info, ok := userFor(user); ok && info.State == bridge.Connected {
		return reply(map[string]any{"user_id": info.UserID})
	}

	p, ok := st.pending[user]
	if !ok {
		var hvDetails *proton.APIHVDetails
		if t := C.GoString(hvToken); t != "" {
			parts := strings.SplitN(t, ":", 2)
			if len(parts) == 2 {
				hvDetails = &proton.APIHVDetails{Methods: []string{parts[0]}, Token: parts[1]}
			}
		}
		pw := []byte(C.GoString(password))
		client, auth, err := st.bridge.LoginAuth(ctx, user, pw, hvDetails)
		if err != nil {
			if hv.IsHvRequest(err) {
				details, _ := hv.VerifyAndExtractHvRequest(err)
				if details != nil {
					return reply(map[string]any{"need": "human_verification", "url": hv.FormatHvURL(details), "token": details.Token})
				}
			}
			if errors.Is(err, bridge.ErrUserAlreadyLoggedIn) {
				if info, ok := userFor(user); ok {
					return reply(map[string]any{"user_id": info.UserID})
				}
			}
			return fail(err)
		}
		p = pendingLogin{client: client, auth: auth, password: pw}
		st.pending[user] = p
	}

	if p.auth.TwoFA.Enabled&proton.HasTOTP != 0 {
		code := strings.TrimSpace(C.GoString(totp))
		if code == "" {
			return reply(map[string]any{"need": "totp"})
		}
		if err := p.client.Auth2FA(ctx, proton.Auth2FAReq{TwoFactorCode: code}); err != nil {
			return reply(map[string]any{"need": "totp", "error": "the two-factor code was not accepted"})
		}
		// Only ask for the code once.
		p.auth.TwoFA.Enabled &^= proton.HasTOTP
		st.pending[user] = p
	}

	keyPass := p.password
	if p.auth.PasswordMode == proton.TwoPasswordMode {
		mp := C.GoString(mailboxPassword)
		if mp == "" {
			return reply(map[string]any{"need": "mailbox_password"})
		}
		keyPass = []byte(mp)
	}

	userID, err := st.bridge.LoginUser(ctx, p.client, p.auth, keyPass, nil)
	if err != nil {
		if errors.Is(err, bridge.ErrFailedToUnlock) {
			return reply(map[string]any{"need": "mailbox_password", "error": "the mailbox password was not accepted"})
		}
		delete(st.pending, user)
		return fail(err)
	}
	delete(st.pending, user)
	_ = st.jar.PersistCookies()
	return reply(map[string]any{"user_id": userID})
}

// moho_bridge_user returns the generated IMAP/SMTP password and the ports for an address.
//
//export moho_bridge_user
func moho_bridge_user(email *C.char) *C.char {
	mu.Lock()
	defer mu.Unlock()
	if st == nil {
		return fail(errors.New("the Proton bridge is not running"))
	}
	info, ok := userFor(C.GoString(email))
	if !ok {
		return reply(map[string]any{"signed_in": false})
	}
	return reply(map[string]any{
		"signed_in":   info.State == bridge.Connected,
		"user_id":     info.UserID,
		"username":    info.Username,
		"addresses":   info.Addresses,
		"bridge_pass": string(info.BridgePass),
		"imap_port":   st.bridge.GetIMAPPort(),
		"smtp_port":   st.bridge.GetSMTPPort(),
	})
}

//export moho_bridge_logout
func moho_bridge_logout(email *C.char) *C.char {
	mu.Lock()
	defer mu.Unlock()
	if st == nil {
		return reply(map[string]any{})
	}
	info, ok := userFor(C.GoString(email))
	if !ok {
		return reply(map[string]any{})
	}
	if err := st.bridge.DeleteUser(context.Background(), info.UserID); err != nil {
		return fail(err)
	}
	return reply(map[string]any{})
}

//export moho_bridge_stop
func moho_bridge_stop() {
	mu.Lock()
	defer mu.Unlock()
	if st == nil {
		return
	}
	_ = st.jar.PersistCookies()
	st.bridge.Close(context.Background())
	_ = st.vault.Close()
	st = nil
}

// Referenced so the linker keeps the crypto package's init for key handling.
var _ = crypto.NewKeyRing
var _ http.CookieJar = (*cookies.Jar)(nil)

func main() {}
