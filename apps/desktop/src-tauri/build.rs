//! Bake the account-service origin into the desktop CSP.
//!
//! Release builds require a valid `SAFI_API_BASE_URL`; debug builds may omit
//! it, but a value that is present is always validated before it reaches the
//! policy. The merged configuration is passed to Tauri through `TAURI_CONFIG`.

use std::path::Path;

use sf_desktop::api_origin::{apply_csp_origin, ApiOrigin, API_ORIGIN_ENV};

fn main() {
    println!("cargo:rerun-if-env-changed={API_ORIGIN_ENV}");
    println!("cargo:rerun-if-changed=tauri.conf.json");

    let profile = std::env::var("PROFILE").unwrap_or_default();
    let configured = std::env::var(API_ORIGIN_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    let origin = match configured {
        Some(value) => Some(ApiOrigin::parse(&value).unwrap_or_else(|error| {
            panic!("{API_ORIGIN_ENV} is invalid: {}", error.message());
        })),
        None if profile == "release" => {
            panic!("{API_ORIGIN_ENV} is required for release desktop builds");
        }
        None => None,
    };

    if let Some(origin) = origin {
        let path = Path::new("tauri.conf.json");
        let raw = std::fs::read_to_string(path).expect("tauri.conf.json must be readable");
        let mut config: serde_json::Value =
            serde_json::from_str(&raw).expect("tauri.conf.json must be valid JSON");
        let current = config["app"]["security"]["csp"]
            .as_str()
            .expect("tauri.conf.json must define app.security.csp")
            .to_owned();
        let merged = apply_csp_origin(&current, &origin)
            .expect("tauri.conf.json must define a non-empty CSP");
        config["app"]["security"]["csp"] = serde_json::Value::String(merged);
        let serialized =
            serde_json::to_string(&config).expect("merged Tauri configuration must serialize");
        // `generate_context!` runs during this crate's compilation, so the
        // merged configuration must reach rustc, not only this build script.
        println!("cargo:rustc-env=TAURI_CONFIG={serialized}");
        std::env::set_var("TAURI_CONFIG", serialized);
    }

    tauri_build::build();
}
