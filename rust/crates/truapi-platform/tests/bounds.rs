//! Compile-time check that the `Platform` super-trait composes its capability
//! traits with `Send + Sync + 'static` bounds. `Platform` itself is not
//! object-safe (the capability traits use `async fn` returning
//! `impl Future`); the runtime consumes implementors via generics, not
//! `dyn Trait`.

use truapi_platform::{Platform, RuntimeConfig, RuntimeConfigValidationError};

fn _assert_platform_bounds<T: Platform + Send + Sync + 'static>() {}

#[test]
fn runtime_config_accepts_https_metadata_url() {
    RuntimeConfig::compatibility_default()
        .validate()
        .expect("compatibility metadata URL is valid https");
}

#[test]
fn runtime_config_rejects_empty_required_fields() {
    let mut config = RuntimeConfig::compatibility_default();
    config.product_label = " ".to_string();
    assert_eq!(
        config.validate(),
        Err(RuntimeConfigValidationError::EmptyField {
            field: "product_label"
        })
    );

    let mut config = RuntimeConfig::compatibility_default();
    config.product_id = String::new();
    assert_eq!(
        config.validate(),
        Err(RuntimeConfigValidationError::EmptyField {
            field: "product_id"
        })
    );

    let mut config = RuntimeConfig::compatibility_default();
    config.site_id = "\t".to_string();
    assert_eq!(
        config.validate(),
        Err(RuntimeConfigValidationError::EmptyField { field: "site_id" })
    );
}

#[test]
fn runtime_config_rejects_relative_metadata_url() {
    let mut config = RuntimeConfig::compatibility_default();
    config.host_metadata_url = "/metadata.json".to_string();

    assert!(matches!(
        config.validate(),
        Err(RuntimeConfigValidationError::InvalidHostMetadataUrl { .. })
    ));
}

#[test]
fn runtime_config_rejects_non_https_metadata_url() {
    let mut config = RuntimeConfig::compatibility_default();
    config.host_metadata_url = "http://localhost:3000/metadata.json".to_string();

    assert_eq!(
        config.validate(),
        Err(RuntimeConfigValidationError::InsecureHostMetadataUrl {
            scheme: "http".to_string(),
        })
    );
}
