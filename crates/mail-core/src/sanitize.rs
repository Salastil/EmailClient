//! HTML email sanitizing. The result is still rendered inside a sandboxed
//! iframe with a strict CSP; this is the first of the two layers.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SanitizedHtml {
    pub html: String,
    /// Number of remote resources removed because remote content is blocked.
    pub blocked_remote: usize,
}

const LAYOUT_ATTRS: &[&str] = &[
    "align", "valign", "width", "height", "bgcolor", "border", "cellpadding", "cellspacing",
    "colspan", "rowspan", "color", "face", "size",
];

fn is_remote(url: &str) -> bool {
    let u = url.trim_start().to_ascii_lowercase();
    u.starts_with("http://") || u.starts_with("https://") || u.starts_with("//")
}

pub fn sanitize_email_html(html: &str, allow_remote: bool) -> SanitizedHtml {
    let blocked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&blocked);

    let mut b = ammonia::Builder::default();
    b.add_generic_attributes(&["style", "dir", "lang", "title"])
        .add_tags(&["center", "font"])
        .add_tag_attributes("img", &["src", "alt", "width", "height"])
        .url_schemes(HashSet::from(["http", "https", "mailto", "cid", "data"]))
        .link_rel(Some("noopener noreferrer nofollow"))
        .attribute_filter(move |element, attribute, value| {
            let loads_resource = matches!(
                (element, attribute),
                ("img", "src") | (_, "background") | (_, "srcset")
            );
            if !allow_remote && loads_resource && is_remote(value) {
                counter.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            if attribute == "src" && value.trim_start().starts_with("data:")
                && !value.trim_start().starts_with("data:image/")
            {
                return None;
            }
            Some(Cow::Borrowed(value))
        });
    for tag in ["table", "tr", "td", "th", "tbody", "thead", "div", "p", "span", "font", "center", "body"] {
        b.add_tag_attributes(tag, LAYOUT_ATTRS);
    }
    for tag in ["td", "th", "table", "body"] {
        b.add_tag_attributes(tag, &["background"]);
    }
    let html = b.clean(html).to_string();
    SanitizedHtml { html, blocked_remote: blocked.load(Ordering::Relaxed) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scripts_and_handlers() {
        let out = sanitize_email_html(
            r#"<p onclick="x()">hi</p><script>alert(1)</script><a href="javascript:x()">l</a>"#,
            false,
        );
        assert!(!out.html.contains("script"));
        assert!(!out.html.contains("onclick"));
        assert!(!out.html.contains("javascript"));
        assert!(out.html.contains("hi"));
    }

    #[test]
    fn blocks_remote_images_unless_allowed() {
        let html = r#"<img src="https://t.example/pixel.gif"><img src="cid:logo"><table><tr><td background="http://x/bg.png">x</td></tr></table>"#;
        let blocked = sanitize_email_html(html, false);
        assert_eq!(blocked.blocked_remote, 2);
        assert!(!blocked.html.contains("pixel.gif"));
        assert!(blocked.html.contains("cid:logo"));

        let allowed = sanitize_email_html(html, true);
        assert_eq!(allowed.blocked_remote, 0);
        assert!(allowed.html.contains("pixel.gif"));
    }

    #[test]
    fn keeps_inline_style_and_table_layout() {
        let out = sanitize_email_html(
            r##"<table width="600" bgcolor="#fff"><tr><td style="color:red" align="center">x</td></tr></table>"##,
            false,
        );
        assert!(out.html.contains(r#"width="600""#));
        assert!(out.html.contains(r#"style="color:red""#));
        assert!(out.html.contains(r#"align="center""#));
    }
}
