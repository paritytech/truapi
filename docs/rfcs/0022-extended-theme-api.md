---
title: "Extended theme subscribe API"
owner: "@filippovecchiato"
---

# RFC 0022 — Extended theme subscribe API

|                 |                                                                  |
| --------------- | ---------------------------------------------------------------- |
| **Start Date**  | 2026-06-01                                                       |
| **Description** | Replace the bare light/dark enum with a structured theme payload carrying a name and variant. |
| **Authors**     | Filippo Vecchiato                                                |

## Summary

Extend `HostThemeSubscribeItem` so the host can tell a product **which** theme to apply, not just whether to use light or dark mode. The subscription delivers a structured value with a theme name and a light/dark variant.

## Motivation

Upstream: [triangle-js-sdks#191](https://github.com/paritytech/triangle-js-sdks/pull/191)

The current `Theme` enum only carries `Light` or `Dark`. Products cannot distinguish named host themes.

## Detailed Design

Replace the existing `Theme` enum with two types:

```rust
/// Identifies a named theme.
enum ThemeName {
    /// A custom named theme.
    Custom(String),
    /// The host's default theme.
    Default,
}

/// Light or dark variant.
enum ThemeVariant {
    Light,
    Dark,
}
```

Update `HostThemeSubscribeItem` to carry both:

```rust
struct HostThemeSubscribeItem {
    /// Theme name.
    pub name: ThemeName,
    /// Light or dark variant.
    pub variant: ThemeVariant,
}
```

The old `Theme` enum (`Light | Dark`) is renamed to `ThemeVariant` for clarity. SCALE codec indices are unchanged — `ThemeVariant::Light` is still index 0, `ThemeVariant::Dark` is still index 1.

`HostThemeSubscribeItem` changes shape (gains a `name` field, `theme: Theme` becomes `variant: ThemeVariant`), so this is a wire-breaking change within v0.1. Host and product must be on matching versions.

## Drawbacks

- Wire-breaking change: deployed products on the old schema will fail to decode the new payload until updated.

