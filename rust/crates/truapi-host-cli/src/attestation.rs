//! Lite-username attestation against the People-chain identity backend.
//!
//! Ports signing-bot `attestation.ts`: fetch the backend verifier, build the
//! client proofs (`truapi_server::host_logic::attestation`), POST them to
//! `/usernames`, then poll People-chain `Resources.Consumers` until the record
//! lands. Registers the signing host's root account so the paired host can
//! resolve its username via `get_user_id`.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use subxt_rpcs::client::{RpcClient, rpc_params};
use tracing::{debug, warn};
use truapi_server::host_logic::attestation::build_lite_registration;
use truapi_server::host_logic::identity::{
    decode_people_identity, resources_consumers_storage_key,
};
use truapi_server::host_logic::product_account::{
    derive_root_keypair_from_entropy, derive_sr25519_hard_path, product_public_key_to_address,
};

/// Inputs for one attestation run.
pub struct AttestConfig {
    /// Identity backend base URL including `/api/v1`.
    pub backend_base: String,
    /// People-chain WebSocket URL for the `Resources.Consumers` poll.
    pub people_ws: String,
    /// BIP-39 entropy of the signing host's root account.
    pub entropy: Vec<u8>,
    /// Requested lite username base (6+ lowercase letters, no digits).
    pub username_base: String,
}

/// Check whether a lite username base is available through the identity
/// backend. The username must be the base form without the digit suffix.
pub async fn lite_username_available(backend_base: &str, username_base: &str) -> Result<bool> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let url = format!("{backend_base}/usernames/available");
    let body = json!({ "usernames": [username_base] });
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?
        .error_for_status()
        .with_context(|| format!("username availability check failed for {username_base}"))?;
    let body: Value = response
        .json()
        .await
        .context("decoding availability response")?;
    Ok(body
        .get(username_base)
        .and_then(Value::as_str)
        .is_some_and(|status| status == "AVAILABLE"))
}

/// Register (or confirm) the signing host's lite username and wait until the
/// People-chain `Resources.Consumers` record exists. Returns the candidate
/// account's SS58 address.
pub async fn attest(config: &AttestConfig) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let verifier = fetch_verifier(&client, &config.backend_base).await?;
    let registration = build_lite_registration(&config.entropy, verifier, &config.username_base)
        .map_err(|reason| anyhow::anyhow!("failed to build registration params: {reason}"))?;
    debug!(
        candidate = %registration.candidate_account_id,
        "attesting lite username '{}'",
        config.username_base
    );

    submit_registration(
        &client,
        &config.backend_base,
        &config.username_base,
        &registration,
    )
    .await?;

    let storage_key = format!(
        "0x{}",
        hex::encode(resources_consumers_storage_key(
            &registration.candidate_public_key
        ))
    );
    wait_for_consumer_record(&config.people_ws, &storage_key).await?;
    debug!("lite username registered and confirmed on-chain");
    Ok(registration.candidate_account_id)
}

/// Probe the People chain for which derivation of `entropy` (bare root,
/// `//wallet`, `//wallet//sso`) has a `Resources.Consumers` record, printing
/// the account and decoded username. Used to confirm a pre-onboarded account.
pub async fn check_identity(people_ws: &str, entropy: &[u8]) -> Result<()> {
    let root = derive_root_keypair_from_entropy(entropy)
        .map_err(|err| anyhow::anyhow!("invalid entropy: {err}"))?;
    let wallet = derive_sr25519_hard_path(entropy, &["wallet"])
        .map_err(|err| anyhow::anyhow!("//wallet derivation failed: {err}"))?;
    let wallet_sso = derive_sr25519_hard_path(entropy, &["wallet", "sso"])
        .map_err(|err| anyhow::anyhow!("//wallet//sso derivation failed: {err}"))?;

    for (label, public) in [
        ("<root>", root.public.to_bytes()),
        ("//wallet", wallet.public.to_bytes()),
        ("//wallet//sso", wallet_sso.public.to_bytes()),
    ] {
        let key = format!(
            "0x{}",
            hex::encode(resources_consumers_storage_key(&public))
        );
        let address = product_public_key_to_address(public);
        match query_storage(people_ws, &key).await {
            Ok(Some(value)) => {
                let decoded = hex::decode(value.strip_prefix("0x").unwrap_or(&value))
                    .ok()
                    .and_then(|bytes| decode_people_identity(&bytes).ok());
                let username = decoded
                    .and_then(|id| id.full_username.or(id.lite_username))
                    .unwrap_or_else(|| "<record present, no username>".to_string());
                println!("IDENTITY_FOUND path={label} account={address} username={username}");
            }
            Ok(None) => println!("IDENTITY_NONE path={label} account={address}"),
            Err(err) => println!("IDENTITY_ERROR path={label} account={address} error={err}"),
        }
    }
    Ok(())
}

async fn fetch_verifier(client: &reqwest::Client, backend_base: &str) -> Result<[u8; 32]> {
    let url = format!("{backend_base}/attester");
    let body: Value = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()?
        .json()
        .await
        .context("decoding attester response")?;
    let hex_value = body
        .get("attester")
        .and_then(Value::as_str)
        .context("attester response missing 'attester' field")?;
    let bytes = hex::decode(hex_value.strip_prefix("0x").unwrap_or(hex_value))
        .context("attester is not valid hex")?;
    <[u8; 32]>::try_from(bytes)
        .map_err(|bytes| anyhow::anyhow!("attester must be 32 bytes, got {}", bytes.len()))
}

async fn submit_registration(
    client: &reqwest::Client,
    backend_base: &str,
    username_base: &str,
    reg: &truapi_server::host_logic::attestation::LiteRegistration,
) -> Result<()> {
    let url = format!("{backend_base}/usernames");
    let body = json!({
        "username": username_base,
        "candidateAccountId": reg.candidate_account_id,
        "candidateSignature": hex0x(&reg.candidate_signature),
        "ringVrfKey": hex0x(&reg.ring_vrf_key),
        "proofOfOwnership": hex0x(&reg.proof_of_ownership),
        "identifierKey": hex0x(&reg.identifier_key),
        "consumerRegistrationSignature": hex0x(&reg.consumer_registration_signature),
    });
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    let status = response.status();
    if status.is_success() {
        let text = response.text().await.unwrap_or_default();
        debug!(%status, body = %text, "POST /usernames accepted");
        return Ok(());
    }
    let text = response.text().await.unwrap_or_default();
    // Already-registered is a soft success; the on-chain poll confirms it.
    if text.contains("already") || text.contains("AlreadyRegistered") || text.contains("duplicate")
    {
        warn!(%status, "username already registered; confirming on-chain");
        return Ok(());
    }
    bail!("username registration failed ({status}): {text}");
}

fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

async fn wait_for_consumer_record(people_ws: &str, storage_key: &str) -> Result<()> {
    // First-time lite registration is backend-async and can take minutes
    // (ring onboarding). The record is permanent once written, so later runs
    // resolve on the first poll.
    const MAX_ATTEMPTS: usize = 90;
    for attempt in 1..=MAX_ATTEMPTS {
        match query_storage(people_ws, storage_key).await {
            Ok(Some(_)) => {
                crate::terminal_ui::update_activity(
                    "signer",
                    "Setting up signer",
                    Some("People-chain identity ready".to_string()),
                    crate::terminal_ui::ActivityState::Running,
                );
                return Ok(());
            }
            Ok(None) => {
                crate::terminal_ui::update_activity(
                    "signer",
                    "Setting up signer",
                    Some(format!(
                        "Waiting for People-chain identity · attempt {attempt}/{MAX_ATTEMPTS}"
                    )),
                    crate::terminal_ui::ActivityState::Running,
                );
                debug!("Resources.Consumers poll {attempt}/{MAX_ATTEMPTS}: empty");
            }
            Err(err) => warn!(%err, "Resources.Consumers poll attempt {attempt} failed"),
        }
        if attempt < MAX_ATTEMPTS {
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
    }
    bail!("Resources.Consumers record did not appear after attestation")
}

/// One `state_getStorage` request over a fresh RPC connection; returns the value
/// hex when present.
async fn query_storage(people_ws: &str, storage_key: &str) -> Result<Option<String>> {
    let rpc = RpcClient::from_insecure_url(people_ws)
        .await
        .with_context(|| format!("connect {people_ws}"))?;
    let value = rpc
        .request::<Value>("state_getStorage", rpc_params![storage_key])
        .await
        .context("rpc state_getStorage")?;
    Ok(value.as_str().map(str::to_string))
}
