//! With the `bundled` feature, compile Proton Mail Bridge (Go) into a static
//! library. PROTON_BRIDGE_SRC must point to a checkout of
//! https://github.com/ProtonMail/proton-bridge at the tag in PROTON_BRIDGE_VERSION.

use std::path::PathBuf;
use std::process::Command;

const DEFAULT_VERSION: &str = "3.27.1";

fn main() {
    println!("cargo:rerun-if-changed=bridge-ffi/main.go");
    println!("cargo:rerun-if-env-changed=PROTON_BRIDGE_SRC");
    println!("cargo:rerun-if-env-changed=PROTON_BRIDGE_VERSION");
    let version = std::env::var("PROTON_BRIDGE_VERSION").unwrap_or_else(|_| DEFAULT_VERSION.into());
    println!("cargo:rustc-env=PROTON_BRIDGE_VERSION={version}");

    if std::env::var_os("CARGO_FEATURE_BUNDLED").is_none() {
        return;
    }

    let src = PathBuf::from(std::env::var("PROTON_BRIDGE_SRC").expect(
        "the `bundled` feature needs PROTON_BRIDGE_SRC pointing to a proton-bridge source checkout",
    ));
    assert!(src.join("go.mod").is_file(), "{} is not a proton-bridge checkout", src.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Go only allows importing a module's internal packages from inside it.
    let ffi_dir = src.join("cmd").join("moho-ffi");
    std::fs::create_dir_all(&ffi_dir).unwrap();
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    std::fs::copy(manifest.join("bridge-ffi/main.go"), ffi_dir.join("main.go")).unwrap();

    let pkg = "github.com/ProtonMail/proton-bridge/v3/internal/constants";
    let ldflags = format!("-s -w -X {pkg}.Version={version} -X '{pkg}.FullAppName=Proton Mail Bridge'");
    let status = Command::new(std::env::var("GO").unwrap_or_else(|_| "go".into()))
        .current_dir(&src)
        .env("CGO_ENABLED", "1")
        // Link the system SQLite instead of go-sqlite3's embedded copy, so Go and
        // Rust share one SQLite in the process (two copies clash at link time).
        .args(["build", "-buildmode=c-archive", "-trimpath", "-mod=mod", "-tags=libsqlite3"])
        .arg(format!("-ldflags={ldflags}"))
        .arg("-o")
        .arg(out.join("libprotonbridge.a"))
        .arg("./cmd/moho-ffi")
        .status()
        .expect("failed to run `go`; install Go to build the Proton bridge");
    assert!(status.success(), "building the Proton bridge failed");

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=protonbridge");
    // C libraries pulled in through cgo (Bridge's keychain helper uses libsecret).
    let pkg = Command::new("pkg-config")
        .args(["--libs", "libsecret-1", "sqlite3"])
        .output()
        .expect("pkg-config is needed to build the Proton bridge");
    assert!(pkg.status.success(), "libsecret-1 or sqlite3 development files are missing");
    for flag in String::from_utf8_lossy(&pkg.stdout).split_whitespace() {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib=dylib={lib}");
        } else if let Some(dir) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={dir}");
        }
    }
    for lib in ["pthread", "dl", "resolv", "m"] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
}
