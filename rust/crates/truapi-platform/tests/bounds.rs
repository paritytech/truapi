//! Compile-time check that the `Platform` super-trait composes its capability
//! traits with `Send + Sync + 'static` bounds. `Platform` itself is not
//! object-safe (the capability traits use `async fn` returning
//! `impl Future`); the runtime consumes implementors via generics, not
//! `dyn Trait`.

use truapi_platform::{
    PairingDeeplinkScheme, Platform, RuntimeConfig, RuntimeConfigValidationError,
};

fn _assert_platform_bounds<T: Platform + Send + Sync + 'static>() {}

fn valid_runtime_config() -> RuntimeConfig {
    runtime_config(
        "dotli.dot",
        "Polkadot Web",
        Some("https://dot.li/dotli.png"),
    )
    .expect("valid runtime config")
}

fn runtime_config(
    product_id: &str,
    host_name: &str,
    host_icon: Option<&str>,
) -> Result<RuntimeConfig, RuntimeConfigValidationError> {
    RuntimeConfig::new(
        product_id.to_string(),
        host_name.to_string(),
        host_icon.map(str::to_string),
        None,
        None,
        None,
        [0xa2; 32],
        PairingDeeplinkScheme::PolkadotApp,
    )
}

#[test]
fn runtime_config_accepts_https_host_icon() {
    valid_runtime_config();
}

#[test]
fn runtime_config_rejects_empty_required_fields() {
    assert_eq!(
        runtime_config("", "Polkadot Web", Some("https://dot.li/dotli.png")),
        Err(RuntimeConfigValidationError::EmptyField {
            field: "product_id"
        })
    );
    assert_eq!(
        runtime_config("dotli.dot", " ", Some("https://dot.li/dotli.png")),
        Err(RuntimeConfigValidationError::EmptyField { field: "host_name" })
    );
}

#[test]
fn runtime_config_rejects_relative_host_icon() {
    assert!(matches!(
        runtime_config("dotli.dot", "Polkadot Web", Some("/dotli.png")),
        Err(RuntimeConfigValidationError::InvalidHostIcon { .. })
    ));
}

#[test]
fn runtime_config_rejects_non_https_host_icon() {
    assert_eq!(
        runtime_config(
            "dotli.dot",
            "Polkadot Web",
            Some("http://localhost:3000/dotli.png")
        ),
        Err(RuntimeConfigValidationError::InsecureHostIcon {
            scheme: "http".to_string(),
        })
    );
}
