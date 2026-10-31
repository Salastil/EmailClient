#!/usr/bin/env python3
"""Test SMTP server for integration tests.

Accepts AUTH PLAIN/LOGIN with any user whose password is "secret" (no TLS,
loopback only) and delivers each message into the recipients' Dovecot
inboxes with `doveadm save`, so sent mail round-trips back over IMAP.
"""
import asyncio
import subprocess
import sys

from aiosmtpd.controller import Controller
from aiosmtpd.smtp import AuthResult, LoginPassword

CONF, PORT = sys.argv[1], int(sys.argv[2])


class Handler:
    async def handle_DATA(self, server, session, envelope):
        for rcpt in envelope.rcpt_tos:
            subprocess.run(
                ["doveadm", "-c", CONF, "save", "-u", rcpt.lower(), "-m", "INBOX"],
                input=envelope.original_content, check=False,
            )
        return "250 Delivered"


def authenticator(server, session, envelope, mechanism, auth_data):
    ok = isinstance(auth_data, LoginPassword) and auth_data.password == b"secret"
    return AuthResult(success=ok)


controller = Controller(
    Handler(), hostname="127.0.0.1", port=PORT,
    authenticator=authenticator, auth_require_tls=False, auth_required=True,
)
controller.start()
asyncio.get_event_loop().run_forever()
