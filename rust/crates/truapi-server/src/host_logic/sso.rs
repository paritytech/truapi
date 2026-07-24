//! Inter-host SSO with the paired wallet: `pairing` bootstraps the
//! QR/deeplink handshake, `messages` carries the session-channel payloads
//! exchanged afterwards.

pub mod messages;
pub mod pairing;
