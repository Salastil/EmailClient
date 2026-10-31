//! Desktop notifications (freedesktop D-Bus). Clicking one opens the message.

use mail_core::model::MessageSummary;
use notify_rust::{Hint, Notification, Timeout};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::window;

#[derive(Clone, Serialize)]
pub struct OpenMessage {
    pub account_id: String,
    pub folder_id: String,
    pub message_id: String,
}

/// More than this many arrivals at once collapse into one summary notification.
const GROUP_AFTER: usize = 3;

fn show(app: &AppHandle, summary: &str, body: &str, target: Option<OpenMessage>) {
    let result = Notification::new()
        .appname("Moho Mail")
        .summary(summary)
        .body(body)
        .icon("moho-mail")
        .hint(Hint::Category("email.arrived".into()))
        .hint(Hint::DesktopEntry("moho-mail".into()))
        .action("default", "Open")
        .timeout(Timeout::Default)
        .show();
    match result {
        Ok(handle) => {
            let app = app.clone();
            std::thread::spawn(move || {
                handle.wait_for_action(|action| {
                    if action == "default" {
                        window::show(&app);
                        if let Some(t) = target {
                            let _ = app.emit("open-message", t);
                        }
                    }
                });
            });
        }
        Err(e) => tracing::warn!("cannot show notification: {e}"),
    }
}

fn escape(s: &str) -> String {
    // Notification bodies may be interpreted as a subset of HTML.
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn show_new_mail(app: &AppHandle, account: &str, msgs: Vec<MessageSummary>) {
    if msgs.is_empty() {
        return;
    }
    if msgs.len() > GROUP_AFTER {
        let body = msgs
            .iter()
            .take(GROUP_AFTER)
            .map(|m| format!("{}: {}", m.from.name.as_deref().unwrap_or(&m.from.email), m.subject))
            .collect::<Vec<_>>()
            .join("\n");
        let first = &msgs[0];
        let target = OpenMessage {
            account_id: first.account_id.clone(),
            folder_id: first.folder_id.clone(),
            message_id: String::new(),
        };
        show(app, &format!("{} new messages · {account}", msgs.len()), &escape(&body), Some(target));
        return;
    }
    for m in msgs {
        let from = m.from.name.clone().unwrap_or_else(|| m.from.email.clone());
        let subject = if m.subject.is_empty() { "(no subject)".to_string() } else { m.subject.clone() };
        let target = OpenMessage { account_id: m.account_id.clone(), folder_id: m.folder_id.clone(), message_id: m.id.clone() };
        show(app, &from, &escape(&subject), Some(target));
    }
}

pub fn show_plain(app: &AppHandle, summary: &str, body: &str) {
    show(app, summary, &escape(body), None);
}
