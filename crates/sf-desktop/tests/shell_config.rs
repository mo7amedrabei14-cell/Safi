//! Conformance tests for the real Tauri 2.2 shell (T5.01).
//!
//! These run without a Tauri toolchain, so CI proves the security posture of the
//! shipped configuration even on machines that cannot build the webview.

use std::collections::BTreeSet;
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn shell_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src-tauri")
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn string_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, Box<dyn Error>> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::other(format!("missing string at {pointer}")).into())
}

fn array_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a Vec<Value>, Box<dyn Error>> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .ok_or_else(|| io::Error::other(format!("missing array at {pointer}")).into())
}

#[test]
fn shell_loads_only_local_content_with_a_strict_csp() -> Result<(), Box<dyn Error>> {
    let config = read_json(&shell_root().join("tauri.conf.json"))?;

    assert_eq!(string_at(&config, "/identifier")?, "app.safidata.desktop");
    assert_eq!(
        string_at(&config, "/build/devUrl")?,
        "http://localhost:1420"
    );
    assert!(
        string_at(&config, "/build/frontendDist")?.ends_with("web/dist"),
        "the shell must embed the real web build"
    );

    assert!(
        config["app"]["windows"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "the single main window is created in Rust so navigation can be guarded"
    );

    let csp = string_at(&config, "/app/security/csp")?;
    assert!(
        csp.contains("default-src 'self'"),
        "CSP must deny by default: {csp}"
    );
    assert!(
        csp.contains("connect-src") && csp.contains("ipc:"),
        "CSP must allow only the Tauri IPC bridge for connections: {csp}"
    );
    assert!(
        !csp.contains("unsafe-inline") && !csp.contains("unsafe-eval"),
        "AGENTS.md rule 11 forbids unsafe-inline and unsafe-eval: {csp}"
    );
    assert_ne!(config["app"]["withGlobalTauri"], Value::Bool(true));

    let capabilities = array_at(&config, "/app/security/capabilities")?;
    assert_eq!(capabilities, &vec![Value::String("default".to_owned())]);

    Ok(())
}

#[test]
fn capabilities_match_the_allowlist_exactly() -> Result<(), Box<dyn Error>> {
    let root = shell_root();
    let capabilities = read_json(&root.join("capabilities/default.json"))?;

    assert_eq!(string_at(&capabilities, "/identifier")?, "default");
    let windows: Vec<&str> = array_at(&capabilities, "/windows")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(windows, vec!["main"]);

    let permissions = array_at(&capabilities, "/permissions")?;
    let mut granted = Vec::new();
    for permission in permissions {
        granted.push(
            permission
                .as_str()
                .ok_or_else(|| io::Error::other("permission must be a string"))?,
        );
    }
    sf_desktop::capabilities::authorize_permissions(granted.iter().copied())?;

    let raw = std::fs::read_to_string(root.join("capabilities/default.json"))?;
    for forbidden in [
        "\"fs:",
        "\"shell:",
        "\"http:",
        "\"process:",
        "\"updater:",
        "\"opener:",
    ] {
        assert!(
            !raw.contains(forbidden),
            "capability file must not grant {forbidden}"
        );
    }

    Ok(())
}

#[test]
fn file_associations_and_deep_link_scheme_are_registered() -> Result<(), Box<dyn Error>> {
    let config = read_json(&shell_root().join("tauri.conf.json"))?;

    let mut extensions = BTreeSet::new();
    for association in array_at(&config, "/bundle/fileAssociations")? {
        for extension in array_at(association, "/ext")? {
            if let Some(extension) = extension.as_str() {
                extensions.insert(extension.to_ascii_lowercase());
            }
        }
    }
    for expected in ["csv", "xls", "xlsx"] {
        assert!(
            extensions.contains(expected),
            "missing file association for {expected}"
        );
    }

    let schemes = array_at(&config, "/plugins/deep-link/desktop/schemes")?;
    assert_eq!(schemes, &vec![Value::String("safi".to_owned())]);

    assert_eq!(
        config["bundle"]["active"],
        Value::Bool(true),
        "installers are active in T5.06"
    );

    Ok(())
}

#[test]
fn shell_wires_single_instance_deep_links_and_the_navigation_guard() -> Result<(), Box<dyn Error>> {
    let source = std::fs::read_to_string(shell_root().join("src/main.rs"))?;

    for needle in [
        "tauri_plugin_single_instance::init",
        "tauri_plugin_deep_link::init",
        "tauri_plugin_dialog::init",
        "sf_desktop::content::validate_app_navigation",
        "sf_desktop::deep_link::parse",
        "on_navigation",
        "WebviewWindowBuilder::new",
        ".devtools(false)",
        "windows_subsystem",
    ] {
        assert!(
            source.contains(needle),
            "main.rs is missing required shell wiring: {needle}"
        );
    }

    Ok(())
}
