//! vCard 3/4 contacts (RFC 6350), reusing the iCalendar line parser.

use serde::{Deserialize, Serialize};

use crate::ical::{escape, parse, unescape};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub uid: String,
    pub name: String,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    #[serde(default)]
    pub organization: String,
    #[serde(default)]
    pub note: String,
}

pub fn contacts(vcf: &str) -> Vec<Contact> {
    parse(vcf)
        .into_iter()
        .filter(|c| c.name == "VCARD")
        .map(|c| {
            let name = c.text("FN").filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
                // N is family;given;additional;prefix;suffix
                c.prop("N")
                    .map(|n| {
                        let parts: Vec<String> = n.value.split(';').map(unescape).collect();
                        format!("{} {}", parts.get(1).cloned().unwrap_or_default(), parts.first().cloned().unwrap_or_default())
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_default()
            });
            Contact {
                uid: c.text("UID").unwrap_or_default(),
                name,
                emails: c.props_named("EMAIL").map(|p| unescape(&p.value).trim().to_lowercase()).filter(|e| e.contains('@')).collect(),
                phones: c.props_named("TEL").map(|p| unescape(&p.value).trim_start_matches("tel:").to_string()).collect(),
                organization: c.text("ORG").map(|o| o.replace(';', " ").trim().to_string()).unwrap_or_default(),
                note: c.text("NOTE").unwrap_or_default(),
            }
        })
        .collect()
}

pub fn to_vcard(c: &Contact) -> String {
    let mut s = String::from("BEGIN:VCARD\r\nVERSION:3.0\r\nPRODID:-//Moho Mail//EN\r\n");
    s.push_str(&format!("UID:{}\r\n", c.uid));
    s.push_str(&format!("FN:{}\r\n", escape(&c.name)));
    let mut parts = c.name.rsplitn(2, ' ');
    let family = parts.next().unwrap_or("");
    let given = parts.next().unwrap_or("");
    s.push_str(&format!("N:{};{};;;\r\n", escape(family), escape(given)));
    for e in &c.emails {
        s.push_str(&format!("EMAIL;TYPE=INTERNET:{}\r\n", escape(e)));
    }
    for p in &c.phones {
        s.push_str(&format!("TEL:{}\r\n", escape(p)));
    }
    if !c.organization.is_empty() {
        s.push_str(&format!("ORG:{}\r\n", escape(&c.organization)));
    }
    if !c.note.is_empty() {
        s.push_str(&format!("NOTE:{}\r\n", escape(&c.note)));
    }
    s.push_str("END:VCARD\r\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_write() {
        let v = "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:u1\r\nN:Doe;Jane;;;\r\nEMAIL;TYPE=work:Jane@Example.com\r\nEMAIL:jd@home.example\r\nTEL;VALUE=uri:tel:+1-555-0100\r\nORG:Acme;R&D\r\nEND:VCARD\r\n";
        let c = &contacts(v)[0];
        assert_eq!(c.name, "Jane Doe");
        assert_eq!(c.emails, vec!["jane@example.com", "jd@home.example"]);
        assert_eq!(c.phones, vec!["+1-555-0100"]);
        assert_eq!(c.organization, "Acme R&D");
        let back = &contacts(&to_vcard(c))[0];
        assert_eq!(back.name, c.name);
        assert_eq!(back.emails, c.emails);
    }
}
