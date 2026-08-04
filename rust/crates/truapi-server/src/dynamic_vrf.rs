//! Bounded dynamic-label Merlin transcript replay for TrUAPI RFC-0023.

use merlin::Transcript;
use schnorrkel::Keypair;

/// Replay runtime-provided labels into a Merlin transcript and sign it.
///
/// Merlin consumes labels synchronously but unnecessarily requires them to be
/// `&'static [u8]`. The lifetime extension is confined to this call: the
/// transcript is created, populated, consumed by `vrf_sign`, and dropped before
/// any input borrow ends. No extended reference can escape this function.
pub(crate) fn sign_dynamic_vrf<'a>(
    keypair: &Keypair,
    transcript_label: &'a [u8],
    items: impl IntoIterator<Item = (&'a [u8], &'a [u8])>,
) -> ([u8; 32], [u8; 64]) {
    let mut transcript = Transcript::new(synchronous_label(transcript_label));
    for (label, value) in items {
        transcript.append_message(synchronous_label(label), value);
    }
    let (in_out, proof, _) = keypair.vrf_sign(transcript);
    (in_out.to_preout().to_bytes(), proof.to_bytes())
}

/// Extend a label only for APIs that consume it synchronously and retain no
/// reference. This helper is private so the extended borrow cannot escape.
#[allow(unsafe_code)]
fn synchronous_label(label: &[u8]) -> &'static [u8] {
    // SAFETY: `Transcript::new` and `Transcript::append_message` absorb the
    // label into STROBE before returning and do not store the slice. The only
    // callers are immediately inside `sign_dynamic_vrf`, whose transcript is
    // consumed before the original label is dropped.
    unsafe { core::mem::transmute::<&[u8], &'static [u8]>(label) }
}
