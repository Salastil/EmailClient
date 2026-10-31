//! Minimal CalDAV (RFC 4791) and CardDAV (RFC 6352) client.

use reqwest::{header, Method, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Auth {
    Basic { user: String, password: String },
    Bearer { token: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Calendar,
    AddressBook,
}

#[derive(Debug, Clone, Serialize)]
pub struct Collection {
    pub url: String,
    pub name: String,
    pub color: Option<String>,
    pub kind: Kind,
    /// Calendar supports VEVENT / VTODO (CalDAV only).
    pub events: bool,
    pub todos: bool,
    pub writable: bool,
}

#[derive(Debug, Clone)]
pub struct Object {
    pub href: String,
    pub etag: String,
    pub data: String,
}

pub struct Client {
    http: reqwest::Client,
    auth: Auth,
}

const NS_DAV: &str = "DAV:";
const NS_CAL: &str = "urn:ietf:params:xml:ns:caldav";
const NS_CARD: &str = "urn:ietf:params:xml:ns:carddav";
const NS_APPLE: &str = "http://apple.com/ns/ical/";

fn propfind_method() -> Method {
    Method::from_bytes(b"PROPFIND").unwrap()
}

fn report_method() -> Method {
    Method::from_bytes(b"REPORT").unwrap()
}

impl Client {
    pub fn new(auth: Auth) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("Moho Mail")
            .timeout(std::time::Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("static client config");
        Self { http, auth }
    }

    fn request(&self, method: Method, url: &str) -> reqwest::RequestBuilder {
        let r = self.http.request(method, url);
        match &self.auth {
            Auth::Basic { user, password } => r.basic_auth(user, Some(password)),
            Auth::Bearer { token } => r.bearer_auth(token),
        }
    }

    async fn xml(&self, method: Method, url: &str, depth: &str, body: &str) -> Result<(String, String)> {
        let resp = self
            .request(method, url)
            .header("Depth", depth)
            .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let final_url = resp.url().to_string();
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(Error::Auth(format!("{status} from {url}")));
        }
        if !status.is_success() {
            return Err(Error::Protocol(format!("{status} from {url}")));
        }
        Ok((final_url, resp.text().await.map_err(|e| Error::Network(e.to_string()))?))
    }

    fn resolve(base: &str, href: &str) -> String {
        Url::parse(base).and_then(|b| b.join(href)).map(String::from).unwrap_or_else(|_| href.to_string())
    }

    /// Follow current-user-principal → home-set and list the collections.
    pub async fn discover(&self, start: &str, kind: Kind) -> Result<Vec<Collection>> {
        let (home_prop, ns) = match kind {
            Kind::Calendar => ("calendar-home-set", NS_CAL),
            Kind::AddressBook => ("addressbook-home-set", NS_CARD),
        };
        let mut start = start.to_string();
        if !start.ends_with('/') && !start.ends_with(".ics") {
            start.push('/');
        }
        // 1. Principal.
        let body = r#"<?xml version="1.0"?><d:propfind xmlns:d="DAV:"><d:prop><d:current-user-principal/><d:resourcetype/></d:prop></d:propfind>"#;
        let mut url = start.clone();
        for candidate in [start.clone(), Self::resolve(&start, match kind { Kind::Calendar => "/.well-known/caldav", Kind::AddressBook => "/.well-known/carddav" })] {
            if let Ok((final_url, text)) = self.xml(propfind_method(), &candidate, "0", body).await {
                url = final_url.clone();
                if let Some(p) = first_href(&text, NS_DAV, "current-user-principal") {
                    url = Self::resolve(&final_url, &p);
                }
                break;
            }
        }
        // 2. Home set (may already be the start URL if it is a collection).
        let body = format!(
            r#"<?xml version="1.0"?><d:propfind xmlns:d="DAV:" xmlns:x="{ns}"><d:prop><x:{home_prop}/></d:prop></d:propfind>"#
        );
        let home = match self.xml(propfind_method(), &url, "0", &body).await {
            Ok((final_url, text)) => first_href(&text, ns, home_prop).map(|h| Self::resolve(&final_url, &h)).unwrap_or(final_url),
            Err(Error::Auth(e)) => return Err(Error::Auth(e)),
            Err(_) => url.clone(),
        };
        // 3. Collections in the home set.
        let body = r#"<?xml version="1.0"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:a="http://apple.com/ns/ical/">
  <d:prop><d:resourcetype/><d:displayname/><a:calendar-color/><c:supported-calendar-component-set/><d:current-user-privilege-set/></d:prop>
</d:propfind>"#;
        let (final_url, text) = self.xml(propfind_method(), &home, "1", body).await?;
        let mut out = parse_collections(&text, &final_url, kind);
        if out.is_empty() {
            // The start URL itself may be a collection (e.g. a direct calendar link).
            let (u, t) = self.xml(propfind_method(), &start, "0", body).await?;
            out = parse_collections(&t, &u, kind);
        }
        Ok(out)
    }

    /// All objects of a collection. Calendars can be limited to a time range.
    pub async fn objects(&self, collection: &Collection, range: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>) -> Result<Vec<Object>> {
        let body = match collection.kind {
            Kind::Calendar => {
                let filter = match range {
                    // Tasks rarely have dates in range, so only filter events.
                    Some((from, to)) if !collection.todos => format!(
                        r#"<c:comp-filter name="VEVENT"><c:time-range start="{}" end="{}"/></c:comp-filter>"#,
                        from.format("%Y%m%dT%H%M%SZ"),
                        to.format("%Y%m%dT%H%M%SZ")
                    ),
                    _ => String::new(),
                };
                format!(
                    r#"<?xml version="1.0"?><c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
<d:prop><d:getetag/><c:calendar-data/></d:prop><c:filter><c:comp-filter name="VCALENDAR">{filter}</c:comp-filter></c:filter></c:calendar-query>"#
                )
            }
            Kind::AddressBook => r#"<?xml version="1.0"?><card:addressbook-query xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
<d:prop><d:getetag/><card:address-data/></d:prop></card:addressbook-query>"#
                .to_string(),
        };
        let (final_url, text) = self.xml(report_method(), &collection.url, "1", &body).await?;
        let data_name = match collection.kind {
            Kind::Calendar => (NS_CAL, "calendar-data"),
            Kind::AddressBook => (NS_CARD, "address-data"),
        };
        Ok(parse_objects(&text, &final_url, data_name))
    }

    /// Create or replace an object. Returns its href and new ETag (if sent).
    pub async fn put(&self, collection_url: &str, href: Option<&str>, uid: &str, data: &str, etag: Option<&str>, kind: Kind) -> Result<(String, String)> {
        let url = match href {
            Some(h) => Self::resolve(collection_url, h),
            None => {
                let ext = if kind == Kind::Calendar { "ics" } else { "vcf" };
                let safe: String = uid.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
                Self::resolve(&ensure_slash(collection_url), &format!("{safe}.{ext}"))
            }
        };
        let ctype = if kind == Kind::Calendar { "text/calendar; charset=utf-8" } else { "text/vcard; charset=utf-8" };
        let mut req = self.request(Method::PUT, &url).header(header::CONTENT_TYPE, ctype).body(data.to_string());
        req = match etag {
            Some(e) if !e.is_empty() => req.header(header::IF_MATCH, e),
            _ if href.is_none() => req.header(header::IF_NONE_MATCH, "*"),
            _ => req,
        };
        let resp = req.send().await.map_err(|e| Error::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Protocol(format!("saving failed: {}", resp.status())));
        }
        let etag = resp.headers().get(header::ETAG).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        Ok((url, etag))
    }

    pub async fn delete(&self, url: &str, etag: Option<&str>) -> Result<()> {
        let mut req = self.request(Method::DELETE, url);
        if let Some(e) = etag.filter(|e| !e.is_empty()) {
            req = req.header(header::IF_MATCH, e);
        }
        let resp = req.send().await.map_err(|e| Error::Network(e.to_string()))?;
        if !resp.status().is_success() && resp.status() != StatusCode::NOT_FOUND {
            return Err(Error::Protocol(format!("delete failed: {}", resp.status())));
        }
        Ok(())
    }

    /// Create a calendar collection (MKCALENDAR), used for tests and "new calendar".
    pub async fn make_calendar(&self, url: &str, name: &str) -> Result<()> {
        let body = format!(
            r#"<?xml version="1.0"?><c:mkcalendar xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:set><d:prop><d:displayname>{name}</d:displayname></d:prop></d:set></c:mkcalendar>"#
        );
        let resp = self
            .request(Method::from_bytes(b"MKCALENDAR").unwrap(), url)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(body)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Protocol(format!("MKCALENDAR failed: {}", resp.status())));
        }
        Ok(())
    }
}

fn ensure_slash(u: &str) -> String {
    if u.ends_with('/') { u.to_string() } else { format!("{u}/") }
}

fn first_href(xml: &str, ns: &str, prop: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    doc.descendants()
        .find(|n| n.tag_name().name() == prop && n.tag_name().namespace() == Some(ns))
        .and_then(|n| n.descendants().find(|c| c.tag_name().name() == "href"))
        .and_then(|h| h.text())
        .map(|t| t.trim().to_string())
}

fn child<'a>(n: roxmltree::Node<'a, 'a>, ns: &str, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    n.descendants().find(|c| c.tag_name().name() == name && c.tag_name().namespace() == Some(ns))
}

fn ok_propstats<'a>(resp: roxmltree::Node<'a, 'a>) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    resp.children().filter(|c| c.tag_name().name() == "propstat").filter(|ps| {
        ps.children()
            .find(|c| c.tag_name().name() == "status")
            .and_then(|s| s.text())
            .is_none_or(|t| t.contains(" 200"))
    })
}

fn parse_collections(xml: &str, base: &str, kind: Kind) -> Vec<Collection> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return vec![] };
    let mut out = Vec::new();
    for resp in doc.descendants().filter(|n| n.tag_name().name() == "response" && n.tag_name().namespace() == Some(NS_DAV)) {
        let Some(href) = resp.children().find(|c| c.tag_name().name() == "href").and_then(|h| h.text()) else { continue };
        let mut is_kind = false;
        let (mut name, mut color, mut events, mut todos, mut writable) = (None, None, true, true, true);
        let mut saw_privs = false;
        let mut saw_comps = false;
        for ps in ok_propstats(resp) {
            if let Some(rt) = child(ps, NS_DAV, "resourcetype") {
                is_kind |= rt.children().any(|c| match kind {
                    Kind::Calendar => c.tag_name().name() == "calendar" && c.tag_name().namespace() == Some(NS_CAL),
                    Kind::AddressBook => c.tag_name().name() == "addressbook" && c.tag_name().namespace() == Some(NS_CARD),
                });
            }
            if let Some(n) = child(ps, NS_DAV, "displayname").and_then(|n| n.text()) {
                name = Some(n.trim().to_string());
            }
            if let Some(c) = child(ps, NS_APPLE, "calendar-color").and_then(|n| n.text()) {
                // Apple colours may be #RRGGBBAA.
                let c = c.trim();
                color = Some(if c.len() == 9 { c[..7].to_string() } else { c.to_string() });
            }
            if let Some(set) = child(ps, NS_CAL, "supported-calendar-component-set") {
                saw_comps = true;
                let comps: Vec<&str> = set.children().filter_map(|c| c.attribute("name")).collect();
                events = comps.contains(&"VEVENT");
                todos = comps.contains(&"VTODO");
            }
            if let Some(privs) = child(ps, NS_DAV, "current-user-privilege-set") {
                saw_privs = true;
                writable = privs.descendants().any(|p| matches!(p.tag_name().name(), "write" | "write-content" | "all" | "bind"));
            }
        }
        if !is_kind {
            continue;
        }
        if !saw_comps && kind == Kind::Calendar {
            events = true;
            todos = false;
        }
        let _ = saw_privs;
        let url = Client::resolve(base, href.trim());
        out.push(Collection {
            name: name.filter(|n| !n.is_empty()).unwrap_or_else(|| {
                url.trim_end_matches('/').rsplit('/').next().unwrap_or("Calendar").to_string()
            }),
            url,
            color,
            kind,
            events,
            todos,
            writable,
        });
    }
    out
}

fn parse_objects(xml: &str, base: &str, (ns, data_name): (&str, &str)) -> Vec<Object> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return vec![] };
    doc.descendants()
        .filter(|n| n.tag_name().name() == "response" && n.tag_name().namespace() == Some(NS_DAV))
        .filter_map(|resp| {
            let href = resp.children().find(|c| c.tag_name().name() == "href")?.text()?.trim().to_string();
            let ps = ok_propstats(resp).next()?;
            let data = child(ps, ns, data_name)?.text()?.to_string();
            let etag = child(ps, NS_DAV, "getetag").and_then(|e| e.text()).unwrap_or("").trim().to_string();
            Some(Object { href: Client::resolve(base, &href), etag, data })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multistatus() {
        let xml = r##"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:a="http://apple.com/ns/ical/">
 <d:response><d:href>/cal/user/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
 <d:response><d:href>/cal/user/work/</d:href><d:propstat><d:prop>
   <d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>Work</d:displayname>
   <a:calendar-color>#FF8800FF</a:calendar-color>
   <c:supported-calendar-component-set><c:comp name="VEVENT"/><c:comp name="VTODO"/></c:supported-calendar-component-set>
   <d:current-user-privilege-set><d:privilege><d:read/></d:privilege></d:current-user-privilege-set>
 </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
</d:multistatus>"##;
        let c = parse_collections(xml, "https://dav.example/cal/user/", Kind::Calendar);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "Work");
        assert_eq!(c[0].url, "https://dav.example/cal/user/work/");
        assert_eq!(c[0].color.as_deref(), Some("#FF8800"));
        assert!(c[0].events && c[0].todos && !c[0].writable);
    }
}
