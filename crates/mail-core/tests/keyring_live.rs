//! Round-trips a secret through the session's real Secret Service. Run it with
//! the keyring daemon stopped to check that it gets started on demand.
//! Skipped unless MOHO_TEST_KEYRING=1 is set.

use mail_core::secrets::{self, SecretError};

#[test]
fn stores_loads_and_deletes_a_secret() {
    if std::env::var("MOHO_TEST_KEYRING").as_deref() != Ok("1") {
        eprintln!("skipped: set MOHO_TEST_KEYRING=1");
        return;
    }
    let name = format!("test:keyring-live:{}", std::process::id());
    secrets::store_named(&name, "hunter2").unwrap();
    assert_eq!(secrets::load_named(&name).unwrap(), "hunter2");
    secrets::delete_named(&name);
    assert!(matches!(secrets::load_named(&name), Err(SecretError::Missing(_))));
}
