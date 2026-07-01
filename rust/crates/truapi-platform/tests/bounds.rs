//! Compile-time check that the `Platform` super-trait composes its capability
//! traits with `Send + Sync + 'static` bounds and remains object-safe via
//! `async_trait`.

use truapi_platform::{
    HostInfo, Platform, PlatformInfo, RuntimeConfig, RuntimeConfigValidationError,
};

fn _assert_platform_bounds<T: Platform + Send + Sync + 'static>() {}

fn _assert_platform_object_safe(_: &(dyn Platform + 'static)) {}

#[test]
fn runtime_config_validation_cases() {
    struct TestCase {
        name: &'static str,
        product_id: &'static str,
        host_name: &'static str,
        host_icon: Option<&'static str>,
        pairing_deeplink_scheme: &'static str,
        expected: Result<(), RuntimeConfigValidationError>,
    }

    let cases = vec![
        TestCase {
            name: "accepts HTTPS host icon",
            product_id: "dotli.dot",
            host_name: "Polkadot Web",
            host_icon: Some("https://dot.li/dotli.png"),
            pairing_deeplink_scheme: "polkadotapp",
            expected: Ok(()),
        },
        TestCase {
            name: "rejects empty product id",
            product_id: "",
            host_name: "Polkadot Web",
            host_icon: Some("https://dot.li/dotli.png"),
            pairing_deeplink_scheme: "polkadotapp",
            expected: Err(RuntimeConfigValidationError::EmptyField {
                field: "product_id",
            }),
        },
        TestCase {
            name: "rejects empty host name",
            product_id: "dotli.dot",
            host_name: " ",
            host_icon: Some("https://dot.li/dotli.png"),
            pairing_deeplink_scheme: "polkadotapp",
            expected: Err(RuntimeConfigValidationError::EmptyField {
                field: "host_info.name",
            }),
        },
        TestCase {
            name: "rejects relative host icon",
            product_id: "dotli.dot",
            host_name: "Polkadot Web",
            host_icon: Some("/dotli.png"),
            pairing_deeplink_scheme: "polkadotapp",
            expected: Err(RuntimeConfigValidationError::InvalidHostIcon {
                reason: "relative URL without a base".to_string(),
            }),
        },
        TestCase {
            name: "rejects non-HTTPS host icon",
            product_id: "dotli.dot",
            host_name: "Polkadot Web",
            host_icon: Some("http://localhost:3000/dotli.png"),
            pairing_deeplink_scheme: "polkadotapp",
            expected: Err(RuntimeConfigValidationError::InsecureHostIcon {
                scheme: "http".to_string(),
            }),
        },
        TestCase {
            name: "rejects malformed deeplink scheme",
            product_id: "dotli.dot",
            host_name: "Polkadot Web",
            host_icon: Some("https://dot.li/dotli.png"),
            pairing_deeplink_scheme: "polkadotapp://",
            expected: Err(RuntimeConfigValidationError::InvalidDeeplinkScheme {
                scheme: "polkadotapp://".to_string(),
            }),
        },
    ];

    for case in cases {
        let result = RuntimeConfig::new(
            case.product_id.to_string(),
            HostInfo {
                name: case.host_name.to_string(),
                icon: case.host_icon.map(str::to_string),
                version: None,
            },
            PlatformInfo::default(),
            [0xa2; 32],
            case.pairing_deeplink_scheme.to_string(),
        )
        .map(|_| ());
        assert_eq!(result, case.expected, "{}", case.name);
    }
}
