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
    RuntimeConfig {
        product_label: "dotli".to_string(),
        product_id: "dotli.dot".to_string(),
        site_id: "dot.li".to_string(),
        host_name: "Polkadot Web".to_string(),
        host_icon: Some("https://dot.li/dotli.png".to_string()),
        host_version: None,
        platform_type: None,
        platform_version: None,
        people_chain_genesis_hash: [0xa2; 32],
        pairing_deeplink_scheme: PairingDeeplinkScheme::PolkadotApp,
    }
}

#[test]
fn runtime_config_accepts_https_host_icon() {
    valid_runtime_config()
        .validate()
        .expect("host icon is valid https");
}

#[test]
fn runtime_config_rejects_empty_required_fields() {
    let mut config = valid_runtime_config();
    config.product_label = " ".to_string();
    assert_eq!(
        config.validate(),
        Err(RuntimeConfigValidationError::EmptyField {
            field: "product_label"
        })
    );

    let mut config = valid_runtime_config();
    config.product_id = String::new();
    assert_eq!(
        config.validate(),
        Err(RuntimeConfigValidationError::EmptyField {
            field: "product_id"
        })
    );

    let mut config = valid_runtime_config();
    config.site_id = "\t".to_string();
    assert_eq!(
        config.validate(),
        Err(RuntimeConfigValidationError::EmptyField { field: "site_id" })
    );
}

#[test]
fn runtime_config_rejects_relative_host_icon() {
    let mut config = valid_runtime_config();
    config.host_icon = Some("/dotli.png".to_string());

    assert!(matches!(
        config.validate(),
        Err(RuntimeConfigValidationError::InvalidHostIcon { .. })
    ));
}

#[test]
fn runtime_config_rejects_non_https_host_icon() {
    let mut config = valid_runtime_config();
    config.host_icon = Some("http://localhost:3000/dotli.png".to_string());

    assert_eq!(
        config.validate(),
        Err(RuntimeConfigValidationError::InsecureHostIcon {
            scheme: "http".to_string(),
        })
    );
}
