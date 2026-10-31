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

pub struct MohoTray {
    app: AppHandle,
    pub unread: u32,
    pub paused: bool,
    /// The desktop prefers a dark theme: draw the envelope light.
    pub dark: bool,
}

pub fn spawn(app: AppHandle) -> Result<TrayHandle, ksni::Error> {
    let handle = MohoTray { app, unread: 0, paused: false, dark: true }.spawn()?;
    tauri::async_runtime::spawn(follow_color_scheme(handle.clone()));
    Ok(handle)
}

/// Track the desktop's light/dark preference (freedesktop appearance portal).
async fn follow_color_scheme(tray: TrayHandle) {
    use ashpd::desktop::settings::{ColorScheme, Settings};
    use futures_util::StreamExt;

    let apply = |scheme: ColorScheme| {
        let tray = tray.clone();
        // Blocking update off the async runtime; see refresh_tray.
        tauri::async_runtime::spawn_blocking(move || {
            tray.update(|t| t.dark = scheme != ColorScheme::PreferLight);
        });
    };
    let Ok(settings) = Settings::new().await else {
        tracing::info!("no appearance portal; the tray icon stays light");
        return;
    };
    if let Ok(scheme) = settings.color_scheme().await {
        apply(scheme);
    }
    let Ok(mut changes) = settings.receive_color_scheme_changed().await else { return };
    while let Some(scheme) = changes.next().await {
        apply(scheme);
    }
}

/// Monochrome envelope in `ink`: an outline when everything is read, filled
/// with a red dot when there is unread mail. Drawn on a 64-unit grid and
/// supersampled so it stays crisp at any size.
fn envelope(size: i32, unread: bool, ink: [u8; 3]) -> Icon {
    fn segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
        let (pa, ba) = ((p.0 - a.0, p.1 - a.1), (b.0 - a.0, b.1 - a.1));
        let t = ((pa.0 * ba.0 + pa.1 * ba.1) / (ba.0 * ba.0 + ba.1 * ba.1)).clamp(0.0, 1.0);
        (pa.0 - ba.0 * t).hypot(pa.1 - ba.1 * t)
    }
    // Signed distance to the envelope body, a rounded rectangle.
    fn body(p: (f32, f32)) -> f32 {
        let (cx, cy, hw, hh, r) = (32.0, 35.0, 27.0, 19.0, 6.0);
        let q = ((p.0 - cx).abs() - hw + r, (p.1 - cy).abs() - hh + r);
        q.0.max(0.0).hypot(q.1.max(0.0)) + q.0.max(q.1).min(0.0) - r
    }
    let flap = |p| segment(p, (7.0, 18.0), (32.0, 37.0)).min(segment(p, (32.0, 37.0), (57.0, 18.0)));
    let dot = (52.0, 14.0, 11.0);
    let in_dot = |p: (f32, f32)| (p.0 - dot.0).hypot(p.1 - dot.1);

    const SS: i32 = 4;
    let scale = 64.0 / size as f32;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (mut env, mut red) = (0.0f32, 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let p = (
                        (x as f32 + (sx as f32 + 0.5) / SS as f32) * scale,
                        (y as f32 + (sy as f32 + 0.5) / SS as f32) * scale,
                    );
                    if unread && in_dot(p) <= dot.2 {
                        red += 1.0;
                        continue;
                    }
                    // Leave a gap around the dot so it reads on any panel.
                    if unread && in_dot(p) <= dot.2 + 3.5 {
                        continue;
                    }
                    let d = body(p);
                    let inked = if unread { d <= 0.0 && flap(p) > 2.5 } else { d.abs() <= 2.75 || (d < 0.0 && flap(p) <= 2.5) };
                    if inked {
                        env += 1.0;
                    }
                }
            }
            let n = (SS * SS) as f32;
            let (env, red) = (env / n, red / n);
            let a = red + env;
            let mix = |r: u8, i: u8| if a > 0.0 { ((r as f32 * red + i as f32 * env) / a).round() as u8 } else { 0 };
            data.extend_from_slice(&[
                (a * 255.0).round() as u8,
                mix(0xe5, ink[0]),
                mix(0x39, ink[1]),
                mix(0x35, ink[2]),
            ]);
        }
    }
    Icon { width: size, height: size, data }
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

    fn icon_pixmap(&self) -> Vec<Icon> {
        let ink = if self.dark { [0xf2, 0xf2, 0xf2] } else { [0x1f, 0x1f, 0x1f] };
        [16, 22, 24, 32, 48, 64].into_iter().map(|size| envelope(size, self.unread > 0, ink)).collect()
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
