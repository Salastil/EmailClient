//! StatusNotifierItem tray icon.
//!
//! Tauri's own tray uses libappindicator on Linux, which never reports a
//! left click. `ksni` speaks the SNI D-Bus protocol directly, so `activate`
//! (left click) can toggle the window on KDE, GNOME with the AppIndicator
//! extension, and wlroots bars.

use chrono::{Duration, Local, NaiveTime};
use ksni::blocking::TrayMethods;
use ksni::menu::{StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::window;

pub type TrayHandle = ksni::blocking::Handle<MohoTray>;

const ICON_PNG: &[u8] = include_bytes!("../icons/64x64.png");

pub struct MohoTray {
    app: AppHandle,
    base_icon: Icon,
    pub unread: u32,
    pub paused: bool,
}

pub fn spawn(app: AppHandle) -> Result<TrayHandle, ksni::Error> {
    let tray = MohoTray { app, base_icon: decode_icon(ICON_PNG), unread: 0, paused: false };
    tray.spawn()
}

/// Convert PNG to the ARGB32 (network byte order) pixmap SNI expects.
fn decode_icon(png: &[u8]) -> Icon {
    let img = tauri::image::Image::from_bytes(png).expect("bundled tray icon is a valid PNG");
    let mut data = img.rgba().to_vec();
    for px in data.chunks_exact_mut(4) {
        px.rotate_right(1);
    }
    Icon { width: img.width() as i32, height: img.height() as i32, data }
}

/// Paint an unread dot in the top-right corner.
fn with_badge(icon: &Icon) -> Icon {
    let mut out = icon.clone();
    let (w, h) = (out.width, out.height);
    let r = w / 5;
    let (cx, cy) = (w - r - 1, r + 1);
    for y in 0..h {
        for x in 0..w {
            let d = (x - cx).pow(2) + (y - cy).pow(2);
            if d <= r * r {
                let i = ((y * w + x) * 4) as usize;
                let rim = d > (r - 2).max(0).pow(2);
                let rgb = if rim { [0x1e, 0x1f, 0x22] } else { [0xf2, 0x3f, 0x42] };
                out.data[i..i + 4].copy_from_slice(&[0xff, rgb[0], rgb[1], rgb[2]]);
            }
        }
    }
    out
}

fn pause_for(app: &AppHandle, until: Option<chrono::DateTime<Local>>) {
    *app.state::<AppState>().paused_until.lock().unwrap() = until;
}

impl ksni::Tray for MohoTray {
    fn id(&self) -> String {
        "moho-mail".into()
    }

    fn title(&self) -> String {
        "Moho Mail".into()
    }

    fn icon_name(&self) -> String {
        // Prefer the themed icon installed by the package; the pixmap is the fallback.
        if self.unread == 0 { "moho-mail".into() } else { String::new() }
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        if self.unread == 0 {
            vec![self.base_icon.clone()]
        } else {
            vec![with_badge(&self.base_icon)]
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let mut description = match self.unread {
            0 => "No unread mail".to_string(),
            1 => "1 unread message".to_string(),
            n => format!("{n} unread messages"),
        };
        if self.paused {
            description.push_str(" · notifications paused");
        }
        ToolTip { title: "Moho Mail".into(), description, ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        window::toggle(&self.app);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let shown = window::is_shown(&self.app);
        vec![
            StandardItem {
                label: if shown { "Hide Moho Mail" } else { "Show Moho Mail" }.into(),
                activate: Box::new(|t: &mut Self| window::toggle(&t.app)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Compose".into(),
                icon_name: "mail-message-new".into(),
                activate: Box::new(|t: &mut Self| {
                    window::show(&t.app);
                    let _ = t.app.emit("compose", ());
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: "Pause notifications".into(),
                submenu: vec![
                    StandardItem {
                        label: "For 1 hour".into(),
                        activate: Box::new(|t: &mut Self| {
                            pause_for(&t.app, Some(Local::now() + Duration::hours(1)));
                            t.paused = true;
                        }),
                        ..Default::default()
                    }
                    .into(),
                    StandardItem {
                        label: "Until tomorrow".into(),
                        activate: Box::new(|t: &mut Self| {
                            let tomorrow = (Local::now() + Duration::days(1))
                                .with_time(NaiveTime::from_hms_opt(8, 0, 0).unwrap())
                                .single();
                            pause_for(&t.app, tomorrow);
                            t.paused = true;
                        }),
                        ..Default::default()
                    }
                    .into(),
                    StandardItem {
                        label: "Resume".into(),
                        enabled: self.paused,
                        activate: Box::new(|t: &mut Self| {
                            pause_for(&t.app, None);
                            t.paused = false;
                        }),
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|t: &mut Self| t.app.exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}
