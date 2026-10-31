#!/usr/bin/env bash
# Start throwaway Dovecot (IMAP) and Radicale (CalDAV/CardDAV) servers on
# loopback for integration tests, then print the environment to export.
#
#   eval "$(scripts/test-servers.sh start)"
#   cargo test --workspace
#   scripts/test-servers.sh stop
set -euo pipefail

DIR="${MOHO_TEST_DIR:-${XDG_RUNTIME_DIR:-/tmp}/moho-mail-test}"
IMAP_PORT=11143
DAV_PORT=15232
SMTP_PORT=11025

start() {
  mkdir -p "$DIR"/{run,state,mail,dav}
  # Dovecot refuses to run login and mail processes as root.
  local login_user mail_user mail_group
  if [ "$(id -u)" = 0 ]; then
    login_user=dovenull; mail_user=nobody; mail_group=nogroup
    chown "$mail_user:$mail_group" "$DIR/mail"
  else
    login_user=$(id -un); mail_user=$(id -un); mail_group=$(id -gn)
  fi
  printf 'test@example.org:{PLAIN}secret\nother@example.org:{PLAIN}secret\n' > "$DIR/users"
  cat > "$DIR/dovecot.conf" <<CONF
base_dir = $DIR/run
state_dir = $DIR/state
log_path = $DIR/dovecot.log
protocols = imap
listen = 127.0.0.1
ssl = no
disable_plaintext_auth = no
auth_mechanisms = plain login
default_login_user = $login_user
default_internal_user = $mail_user
default_internal_group = $mail_group
passdb {
  driver = passwd-file
  args = scheme=PLAIN username_format=%u $DIR/users
}
userdb {
  driver = static
  args = uid=$mail_user gid=$mail_group home=$DIR/mail/%u
}
mail_location = maildir:~/Maildir
namespace inbox {
  inbox = yes
  separator = /
  mailbox Sent {
    special_use = \\Sent
    auto = create
  }
  mailbox Drafts {
    special_use = \\Drafts
    auto = create
  }
  mailbox Trash {
    special_use = \\Trash
    auto = create
  }
  mailbox Junk {
    special_use = \\Junk
    auto = create
  }
}
service imap-login {
  inet_listener imap {
    port = $IMAP_PORT
  }
  inet_listener imaps {
    port = 0
  }
  chroot =
}
service anvil {
  chroot =
}
CONF
  dovecot -c "$DIR/dovecot.conf"
  if python3 -c "import aiosmtpd" 2>/dev/null; then
    python3 "$(dirname "$0")/smtp-sink.py" "$DIR/dovecot.conf" "$SMTP_PORT" >"$DIR/smtp.log" 2>&1 &
    echo $! > "$DIR/smtp.pid"
    echo "export MOHO_TEST_SMTP=127.0.0.1:$SMTP_PORT"
  fi
  if command -v radicale >/dev/null; then
    printf 'test:secret\n' > "$DIR/dav-users"
    radicale --server-hosts "127.0.0.1:$DAV_PORT" --storage-filesystem-folder "$DIR/dav" \
      --auth-type htpasswd --auth-htpasswd-filename "$DIR/dav-users" --auth-htpasswd-encryption plain \
      >"$DIR/radicale.log" 2>&1 &
    echo $! > "$DIR/radicale.pid"
    echo "export MOHO_TEST_DAV=http://127.0.0.1:$DAV_PORT/"
  fi
  echo "export MOHO_TEST_IMAP=127.0.0.1:$IMAP_PORT"
  echo "export MOHO_TEST_DIR=$DIR"
}

stop() {
  doveadm -c "$DIR/dovecot.conf" stop 2>/dev/null || true
  for p in radicale smtp; do
    [ -f "$DIR/$p.pid" ] && kill "$(cat "$DIR/$p.pid")" 2>/dev/null || true
  done
  rm -rf "$DIR"
}

"${1:-start}"
