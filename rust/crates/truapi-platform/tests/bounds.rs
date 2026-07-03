//! Compile-time check that the `Platform` super-trait composes its capability
//! traits with `Send + Sync + 'static` bounds and remains object-safe via
//! `async_trait`.

use truapi_platform::{
    HostInfo, HostRuntimeConfig, PairingHostConfig, Platform, PlatformInfo, ProductContext,
    RuntimeConfigValidationError,
};

fn _assert_platform_bounds<T: Platform + Send + Sync + 'static>() {}

fn _assert_platform_object_safe(_: &(dyn Platform + 'static)) {}

#[test]
fn runtime_config_validation_cases() {
    struct TestCase {
        name: &'static str,
        host_name: &'static str,
        host_icon: Option<&'static str>,
        expected: Result<(), RuntimeConfigValidationError>,
    }

    let cases = vec![
        TestCase {
            name: "accepts HTTPS host icon",
            host_name: "Polkadot Web",
            host_icon: Some("https://dot.li/dotli.png"),
            expected: Ok(()),
        },
        TestCase {
            name: "rejects empty host name",
            host_name: " ",
            host_icon: Some("https://dot.li/dotli.png"),
            expected: Err(RuntimeConfigValidationError::EmptyField {
                field: "host_info.name",
            }),
        },
        TestCase {
            name: "rejects relative host icon",
            host_name: "Polkadot Web",
            host_icon: Some("/dotli.png"),
            expected: Err(RuntimeConfigValidationError::InvalidHostIcon {
                reason: "relative URL without a base".to_string(),
            }),
        },
        TestCase {
            name: "rejects non-HTTPS host icon",
            host_name: "Polkadot Web",
            host_icon: Some("http://localhost:3000/dotli.png"),
            expected: Err(RuntimeConfigValidationError::InsecureHostIcon {
                scheme: "http".to_string(),
            }),
        },
    ];

    for case in cases {
        let result = HostRuntimeConfig::new(
            HostInfo {
                name: case.host_name.to_string(),
                icon: case.host_icon.map(str::to_string),
                version: None,
            },
            PlatformInfo::default(),
        )
        .map(|_| ());
        assert_eq!(result, case.expected, "{}", case.name);
    }
}

#[test]
fn pairing_config_validation_cases() {
    struct TestCase {
        name: &'static str,
        host_name: &'static str,
        host_icon: Option<&'static str>,
        pairing_deeplink_scheme: &'static str,
        expected: Result<(), RuntimeConfigValidationError>,
    }

    let cases = vec![TestCase {
        name: "rejects malformed deeplink scheme",
        host_name: "Polkadot Web",
        host_icon: Some("https://dot.li/dotli.png"),
        pairing_deeplink_scheme: "polkadotapp://",
        expected: Err(RuntimeConfigValidationError::InvalidDeeplinkScheme {
            scheme: "polkadotapp://".to_string(),
        }),
    }];

    for case in cases {
        let result = PairingHostConfig::new(
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

#[test]
fn product_context_validation_cases() {
    let dotli = ProductContext::new("Dotli.DOT".to_string()).expect("dot product id is valid");
    assert_eq!(dotli.product_id, "dotli.dot");

    let localhost =
        ProductContext::new(" localhost:3000 ".to_string()).expect("localhost product id is valid");
    assert_eq!(localhost.product_id, "localhost:3000");

    assert_eq!(
        ProductContext::new("localhost".to_string()).map(|context| context.product_id),
        Ok("localhost".to_string())
    );
    assert_eq!(
        ProductContext::new("dotli.dot".to_string()).map(|_| ()),
        Ok(())
    );
    assert_eq!(
        ProductContext::new("example.com".to_string()).map(|_| ()),
        Err(RuntimeConfigValidationError::InvalidProductId {
            product_id: "example.com".to_string(),
        })
    );
    assert_eq!(
        ProductContext::new(" ".to_string()).map(|_| ()),
        Err(RuntimeConfigValidationError::EmptyField {
            field: "product_id",
        })
    );
}
