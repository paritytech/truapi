//! Conversions between the TrUAPI host types (`Statement`, `SignedStatement`,
//! `StatementProof`) and the polkadot-sdk `sp_statement_store` types.
//!
//! Ports `web/host/packages/ui/src/statement-store-mapping.ts`. Hosts that
//! bridge TrUAPI wire frames to a real statement store use `sp.into()` for the
//! infallible sdk → host direction and `host.try_into()?` for the fallible
//! host → sdk direction (fails when the host supplied more than
//! `sp_statement_store::MAX_TOPICS` topics).
//!
//! Gated on the `sp-compat` feature so WASM builds of `truapi` stay slim.

use sp_statement_store::{self as sp, MAX_TOPICS};

use crate::v01::{SignedStatement, Statement, StatementProof};

/// Reason a `Statement` / `SignedStatement` cannot be mapped into an
/// `sp_statement_store::Statement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementMappingError {
    /// Host supplied more than `sp_statement_store::MAX_TOPICS` topics.
    TooManyTopics {
        /// Number of topics the host asked for.
        got: usize,
        /// Limit enforced by `sp_statement_store`.
        max: usize,
    },
}

impl core::fmt::Display for StatementMappingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooManyTopics { got, max } => {
                write!(f, "too many topics: got {got}, max {max}")
            }
        }
    }
}

impl std::error::Error for StatementMappingError {}

// ─── Proof ────────────────────────────────────────────────────────────────

impl From<StatementProof> for sp::Proof {
    fn from(proof: StatementProof) -> Self {
        match proof {
            StatementProof::Sr25519 { signature, signer } => {
                sp::Proof::Sr25519 { signature, signer }
            }
            StatementProof::Ed25519 { signature, signer } => {
                sp::Proof::Ed25519 { signature, signer }
            }
            StatementProof::Ecdsa { signature, signer } => {
                sp::Proof::Secp256k1Ecdsa { signature, signer }
            }
            StatementProof::OnChain {
                who,
                block_hash,
                event,
            } => sp::Proof::OnChain {
                who,
                block_hash,
                event_index: event,
            },
        }
    }
}

impl From<sp::Proof> for StatementProof {
    fn from(proof: sp::Proof) -> Self {
        match proof {
            sp::Proof::Sr25519 { signature, signer } => {
                StatementProof::Sr25519 { signature, signer }
            }
            sp::Proof::Ed25519 { signature, signer } => {
                StatementProof::Ed25519 { signature, signer }
            }
            sp::Proof::Secp256k1Ecdsa { signature, signer } => {
                StatementProof::Ecdsa { signature, signer }
            }
            sp::Proof::OnChain {
                who,
                block_hash,
                event_index,
            } => StatementProof::OnChain {
                who,
                block_hash,
                event: event_index,
            },
        }
    }
}

// ─── Statement ────────────────────────────────────────────────────────────

fn build_sp_statement(
    proof: Option<StatementProof>,
    decryption_key: Option<[u8; 32]>,
    expiry: Option<u64>,
    channel: Option<[u8; 32]>,
    topics: Vec<[u8; 32]>,
    data: Option<Vec<u8>>,
) -> Result<sp::Statement, StatementMappingError> {
    if topics.len() > MAX_TOPICS {
        return Err(StatementMappingError::TooManyTopics {
            got: topics.len(),
            max: MAX_TOPICS,
        });
    }
    let mut out = sp::Statement::new();
    if let Some(p) = proof {
        out.set_proof(p.into());
    }
    if let Some(key) = decryption_key {
        out.set_decryption_key(key);
    }
    if let Some(exp) = expiry {
        out.set_expiry(exp);
    }
    if let Some(ch) = channel {
        out.set_channel(ch);
    }
    for (index, topic) in topics.into_iter().enumerate() {
        out.set_topic(index, sp::Topic::from(topic));
    }
    if let Some(bytes) = data {
        out.set_plain_data(bytes);
    }
    Ok(out)
}

impl TryFrom<Statement> for sp::Statement {
    type Error = StatementMappingError;

    fn try_from(statement: Statement) -> Result<Self, Self::Error> {
        build_sp_statement(
            statement.proof,
            statement.decryption_key,
            statement.expiry,
            statement.channel,
            statement.topics,
            statement.data,
        )
    }
}

impl TryFrom<SignedStatement> for sp::Statement {
    type Error = StatementMappingError;

    fn try_from(statement: SignedStatement) -> Result<Self, Self::Error> {
        build_sp_statement(
            Some(statement.proof),
            statement.decryption_key,
            statement.expiry,
            statement.channel,
            statement.topics,
            statement.data,
        )
    }
}

impl From<sp::Statement> for Statement {
    fn from(statement: sp::Statement) -> Self {
        let proof = statement.proof().cloned().map(Into::into);
        let decryption_key = statement.decryption_key();
        // `sp_statement_store::Statement` always holds `expiry: u64` with 0
        // meaning "unset" (the field is skipped on the wire when zero). Host
        // semantics keep expiry optional, so collapse 0 back to `None`.
        let expiry = match statement.expiry() {
            0 => None,
            n => Some(n),
        };
        let channel = statement.channel();
        let topics = statement
            .topics()
            .iter()
            .map(|t| <[u8; 32]>::from(*t))
            .collect();
        let data = statement.data().cloned();
        Statement {
            proof,
            decryption_key,
            expiry,
            channel,
            topics,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::{Decode, Encode};

    fn round_trip_statement(statement: Statement) {
        let sp_stmt: sp::Statement = statement.clone().try_into().unwrap();
        let back: Statement = sp_stmt.into();
        assert_eq!(back, statement);
    }

    #[test]
    fn proof_variants_round_trip() {
        let proofs = [
            StatementProof::Sr25519 {
                signature: [1u8; 64],
                signer: [2u8; 32],
            },
            StatementProof::Ed25519 {
                signature: [3u8; 64],
                signer: [4u8; 32],
            },
            StatementProof::Ecdsa {
                signature: [5u8; 65],
                signer: [6u8; 33],
            },
            StatementProof::OnChain {
                who: [7u8; 32],
                block_hash: [8u8; 32],
                event: 42,
            },
        ];
        for p in proofs {
            let sp_p: sp::Proof = p.clone().into();
            let back: StatementProof = sp_p.into();
            assert_eq!(back, p);
        }
    }

    #[test]
    fn statement_with_all_fields_round_trips() {
        round_trip_statement(Statement {
            proof: Some(StatementProof::Sr25519 {
                signature: [9u8; 64],
                signer: [10u8; 32],
            }),
            decryption_key: Some([11u8; 32]),
            expiry: Some(123_456),
            channel: Some([12u8; 32]),
            topics: vec![[13u8; 32], [14u8; 32], [15u8; 32], [16u8; 32]],
            data: Some(b"hello".to_vec()),
        });
    }

    #[test]
    fn empty_statement_round_trips() {
        round_trip_statement(Statement {
            proof: None,
            decryption_key: None,
            expiry: None,
            channel: None,
            topics: Vec::new(),
            data: None,
        });
    }

    #[test]
    fn too_many_topics_rejected() {
        let statement = Statement {
            proof: None,
            decryption_key: None,
            expiry: None,
            channel: None,
            topics: vec![[0u8; 32]; MAX_TOPICS + 1],
            data: None,
        };
        let err = sp::Statement::try_from(statement).unwrap_err();
        assert_eq!(
            err,
            StatementMappingError::TooManyTopics {
                got: MAX_TOPICS + 1,
                max: MAX_TOPICS,
            }
        );
    }

    #[test]
    fn signed_statement_maps_with_required_proof() {
        let signed = SignedStatement {
            proof: StatementProof::Ed25519 {
                signature: [1u8; 64],
                signer: [2u8; 32],
            },
            decryption_key: None,
            expiry: Some(100),
            channel: None,
            topics: vec![[3u8; 32]],
            data: Some(b"x".to_vec()),
        };
        let sp_stmt: sp::Statement = signed.try_into().unwrap();
        assert!(matches!(sp_stmt.proof(), Some(sp::Proof::Ed25519 { .. })));
        assert_eq!(sp_stmt.expiry(), 100);
        assert_eq!(sp_stmt.data(), Some(&b"x".to_vec()));
    }

    #[test]
    fn wire_encoding_matches_sp_encoding() {
        // Encode via sp::Statement, decode via sp::Statement — confirms the
        // two sides agree on SCALE bytes for a representative statement.
        let host = Statement {
            proof: Some(StatementProof::Sr25519 {
                signature: [42u8; 64],
                signer: [7u8; 32],
            }),
            decryption_key: None,
            expiry: Some(1_700_000_000),
            channel: None,
            topics: vec![[1u8; 32], [2u8; 32]],
            data: Some(b"payload".to_vec()),
        };
        let sp_stmt: sp::Statement = host.clone().try_into().unwrap();
        let bytes = sp_stmt.encode();
        let decoded = sp::Statement::decode(&mut &bytes[..]).unwrap();
        let back: Statement = decoded.into();
        assert_eq!(back, host);
    }

    #[test]
    fn zero_expiry_collapses_to_none() {
        let mut sp_stmt = sp::Statement::new();
        sp_stmt.set_expiry(0);
        let host: Statement = sp_stmt.into();
        assert_eq!(host.expiry, None);
    }
}
