//! `StatementStore` surface: session-key statement proofs plus submit and
//! subscribe flows over the people-chain statement store.

use std::pin::Pin;
use std::task::{Context, Poll};

use super::PlatformRuntimeHost;
use crate::host_logic::statement_store::{
    MAX_MATCH_ALL_TOPICS, MAX_MATCH_ANY_TOPICS, TopicFilterKind, decode_signed_statement,
    parse_new_statements, parse_submit_ack, parse_subscribe_ack, sign_statement_fields,
    signed_statement_to_scale, statement_fields_from_v01, statement_proof_to_v01,
    submit_statement_request, subscribe_match_all_request, subscribe_match_any_request,
    unsubscribe_request,
};

use futures::StreamExt;
use futures::stream::BoxStream;
use tracing::instrument;
use truapi::api::StatementStore;
use truapi::v01;
use truapi::versioned::statement_store::{
    RemoteStatementStoreCreateProofAuthorizedError,
    RemoteStatementStoreCreateProofAuthorizedRequest,
    RemoteStatementStoreCreateProofAuthorizedResponse, RemoteStatementStoreCreateProofError,
    RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
    RemoteStatementStoreSubmitError, RemoteStatementStoreSubmitRequest,
    RemoteStatementStoreSubscribeError, RemoteStatementStoreSubscribeItem,
    RemoteStatementStoreSubscribeRequest,
};
use truapi::{CallContext, CallError, Subscription};
use truapi_platform::{ChainProvider as PlatformChainProvider, JsonRpcConnection, Platform};

impl<P> StatementStore for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "statement_store.subscribe"))]
    async fn subscribe(
        &self,
        cx: &CallContext,
        request: RemoteStatementStoreSubscribeRequest,
    ) -> Result<
        Subscription<RemoteStatementStoreSubscribeItem>,
        CallError<RemoteStatementStoreSubscribeError>,
    > {
        let (kind, topics) = match statement_store_topic_filter(request) {
            Ok(value) => value,
            Err(reason) => {
                return Err(CallError::Domain(RemoteStatementStoreSubscribeError::V1(
                    v01::GenericError { reason },
                )));
            }
        };
        let request_id = if cx.request_id().is_empty() {
            "truapi:ss-subscribe".to_string()
        } else {
            cx.request_id().to_string()
        };
        let connection = match PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .await
        {
            Ok(connection) => connection,
            Err(err) => {
                return Err(CallError::Domain(RemoteStatementStoreSubscribeError::V1(
                    v01::GenericError {
                        reason: format!("statement-store connect failed: {err:?}"),
                    },
                )));
            }
        };
        connection.send(match kind {
            TopicFilterKind::MatchAll => subscribe_match_all_request(&request_id, &topics),
            TopicFilterKind::MatchAny => subscribe_match_any_request(&request_id, &topics),
        });
        let responses = connection.responses();
        let stream = statement_store_subscription_stream(connection, responses, request_id);
        Ok(Subscription::new(Box::pin(stream)))
    }

    #[instrument(skip_all, fields(runtime.method = "statement_store.create_proof"))]
    async fn create_proof(
        &self,
        _cx: &CallContext,
        request: RemoteStatementStoreCreateProofRequest,
    ) -> Result<
        RemoteStatementStoreCreateProofResponse,
        CallError<RemoteStatementStoreCreateProofError>,
    > {
        let RemoteStatementStoreCreateProofRequest::V1(mut inner) = request;
        inner.product_account_id = Self::normalize_product_account_id(inner.product_account_id);
        if !self.is_product_account_valid_for_caller(&inner.product_account_id.dot_ns_identifier) {
            return Err(CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::UnknownAccount,
            )));
        }
        let proof = self
            .create_statement_proof(inner.statement)
            .map_err(statement_proof_error)?;
        Ok(RemoteStatementStoreCreateProofResponse::V1(
            v01::RemoteStatementStoreCreateProofResponse { proof },
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "statement_store.create_proof_authorized"))]
    async fn create_proof_authorized(
        &self,
        _cx: &CallContext,
        request: RemoteStatementStoreCreateProofAuthorizedRequest,
    ) -> Result<
        RemoteStatementStoreCreateProofAuthorizedResponse,
        CallError<RemoteStatementStoreCreateProofAuthorizedError>,
    > {
        let RemoteStatementStoreCreateProofAuthorizedRequest::V1(statement) = request;
        let proof = self
            .create_statement_proof(statement)
            .map_err(statement_proof_authorized_error)?;
        Ok(RemoteStatementStoreCreateProofAuthorizedResponse::V1(
            v01::RemoteStatementStoreCreateProofResponse { proof },
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "statement_store.submit"))]
    async fn submit(
        &self,
        cx: &CallContext,
        request: RemoteStatementStoreSubmitRequest,
    ) -> Result<(), CallError<RemoteStatementStoreSubmitError>> {
        let RemoteStatementStoreSubmitRequest::V1(statement) = request;
        let statement = signed_statement_to_scale(statement).map_err(|reason| {
            CallError::Domain(RemoteStatementStoreSubmitError::V1(v01::GenericError {
                reason,
            }))
        })?;
        let request_id = if cx.request_id().is_empty() {
            "truapi:ss-submit".to_string()
        } else {
            cx.request_id().to_string()
        };
        let connection = PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .await
        .map_err(|err| {
            CallError::Domain(RemoteStatementStoreSubmitError::V1(v01::GenericError {
                reason: format!("statement-store connect failed: {err:?}"),
            }))
        })?;
        connection.send(submit_statement_request(&request_id, &statement));
        wait_for_statement_submit_ack(connection.responses(), &request_id)
            .await
            .map_err(|reason| {
                CallError::Domain(RemoteStatementStoreSubmitError::V1(v01::GenericError {
                    reason,
                }))
            })
    }
}

fn statement_store_topic_filter(
    request: RemoteStatementStoreSubscribeRequest,
) -> Result<(TopicFilterKind, Vec<[u8; 32]>), String> {
    match request {
        RemoteStatementStoreSubscribeRequest::V1(
            v01::RemoteStatementStoreSubscribeRequest::MatchAll(topics),
        ) => {
            if topics.len() > MAX_MATCH_ALL_TOPICS {
                return Err(format!(
                    "MatchAll has {} topics, maximum is {}",
                    topics.len(),
                    MAX_MATCH_ALL_TOPICS
                ));
            }
            Ok((TopicFilterKind::MatchAll, topics))
        }
        RemoteStatementStoreSubscribeRequest::V1(
            v01::RemoteStatementStoreSubscribeRequest::MatchAny(topics),
        ) => {
            if topics.len() > MAX_MATCH_ANY_TOPICS {
                return Err(format!(
                    "MatchAny has {} topics, maximum is {}",
                    topics.len(),
                    MAX_MATCH_ANY_TOPICS
                ));
            }
            Ok((TopicFilterKind::MatchAny, topics))
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "statement_store.wait_submit_ack"))]
async fn wait_for_statement_submit_ack(
    mut responses: BoxStream<'static, String>,
    request_id: &str,
) -> Result<(), String> {
    while let Some(frame) = responses.next().await {
        if parse_submit_ack(&frame, request_id)
            .map_err(|err| err.to_string())?
            .is_some()
        {
            return Ok(());
        }
    }
    Err("statement-store submit response stream ended".to_string())
}

fn statement_store_subscription_stream(
    connection: Box<dyn JsonRpcConnection>,
    responses: BoxStream<'static, String>,
    request_id: String,
) -> impl futures::Stream<Item = RemoteStatementStoreSubscribeItem> + Send {
    StatementStoreSubscriptionStream {
        connection,
        responses,
        request_id,
        remote_subscription_id: None,
        is_complete: false,
    }
}

struct StatementStoreSubscriptionStream {
    connection: Box<dyn JsonRpcConnection>,
    responses: BoxStream<'static, String>,
    request_id: String,
    remote_subscription_id: Option<String>,
    is_complete: bool,
}

impl Unpin for StatementStoreSubscriptionStream {}

impl futures::Stream for StatementStoreSubscriptionStream {
    type Item = RemoteStatementStoreSubscribeItem;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let state = self.get_mut();
        loop {
            let frame = match state.responses.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(frame)) => frame,
                Poll::Ready(None) => return Poll::Ready(None),
            };

            if state.remote_subscription_id.is_none() {
                match parse_subscribe_ack(&frame, &state.request_id) {
                    Ok(Some(id)) => {
                        state.remote_subscription_id = Some(id);
                        continue;
                    }
                    Ok(None) => {}
                    Err(_) => return Poll::Ready(None),
                }
            }

            let page = match parse_new_statements(&frame) {
                Ok(Some(page)) => page,
                Ok(None) => continue,
                Err(_) => return Poll::Ready(None),
            };
            // Only accept pages for the acked subscription id; pages that
            // arrive before the subscribe ack cannot be attributed and are
            // dropped.
            if state.remote_subscription_id.as_deref() != Some(page.remote_subscription_id.as_str())
            {
                continue;
            }

            let was_complete = state.is_complete;
            let is_complete = was_complete || page.remaining == Some(0);
            state.is_complete = is_complete;
            let statements = page
                .statements
                .into_iter()
                .filter_map(|statement| decode_signed_statement(&statement).ok())
                .collect::<Vec<_>>();
            if statements.is_empty() {
                if is_complete && !was_complete {
                    return Poll::Ready(Some(RemoteStatementStoreSubscribeItem::V1(
                        v01::RemoteStatementStoreSubscribeItem {
                            statements,
                            is_complete,
                        },
                    )));
                }
                continue;
            }

            return Poll::Ready(Some(RemoteStatementStoreSubscribeItem::V1(
                v01::RemoteStatementStoreSubscribeItem {
                    statements,
                    is_complete,
                },
            )));
        }
    }
}

impl Drop for StatementStoreSubscriptionStream {
    fn drop(&mut self) {
        if let Some(remote_subscription_id) = self.remote_subscription_id.as_ref() {
            self.connection.send(unsubscribe_request(
                &format!("{}:unsubscribe", self.request_id),
                remote_subscription_id,
            ));
        }
    }
}

impl<P> PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    fn create_statement_proof(
        &self,
        statement: v01::Statement,
    ) -> Result<v01::StatementProof, StatementProofFailure> {
        let session = self
            .session_state
            .current()
            .ok_or(StatementProofFailure::NoSession)?;
        let sso = session
            .sso
            .as_ref()
            .ok_or(StatementProofFailure::NoSession)?;
        let fields = statement_fields_from_v01(statement)
            .map_err(StatementProofFailure::InvalidStatement)?;
        let signed = sign_statement_fields(sso.ss_secret, sso.ss_public_key, fields)
            .map_err(StatementProofFailure::UnableToSign)?;
        signed
            .into_iter()
            .find_map(|field| match field {
                crate::host_logic::statement_store::StatementField::Proof(proof) => {
                    Some(statement_proof_to_v01(proof))
                }
                _ => None,
            })
            .ok_or_else(|| StatementProofFailure::UnableToSign("missing proof".to_string()))
    }
}

enum StatementProofFailure {
    NoSession,
    InvalidStatement(String),
    UnableToSign(String),
}

fn statement_proof_error(
    failure: StatementProofFailure,
) -> CallError<RemoteStatementStoreCreateProofError> {
    match failure {
        StatementProofFailure::NoSession => {
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::UnableToSign,
            ))
        }
        StatementProofFailure::UnableToSign(_reason) => {
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::UnableToSign,
            ))
        }
        StatementProofFailure::InvalidStatement(reason) => {
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::Unknown { reason },
            ))
        }
    }
}

fn statement_proof_authorized_error(
    failure: StatementProofFailure,
) -> CallError<RemoteStatementStoreCreateProofAuthorizedError> {
    match failure {
        StatementProofFailure::NoSession => {
            CallError::Domain(RemoteStatementStoreCreateProofAuthorizedError::V1(
                v01::RemoteStatementStoreCreateProofError::UnableToSign,
            ))
        }
        StatementProofFailure::UnableToSign(_reason) => {
            CallError::Domain(RemoteStatementStoreCreateProofAuthorizedError::V1(
                v01::RemoteStatementStoreCreateProofError::UnableToSign,
            ))
        }
        StatementProofFailure::InvalidStatement(reason) => {
            CallError::Domain(RemoteStatementStoreCreateProofAuthorizedError::V1(
                v01::RemoteStatementStoreCreateProofError::Unknown { reason },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::{
        StubPlatform, account_id, new_statements_frame, runtime_config, signed_statement,
        sso_session_info, statement, stub_platform, subscribe_ack_frame, test_spawner,
    };
    use parity_scale_codec::Encode;
    use std::sync::Arc;

    #[test]
    fn statement_store_create_proof_signs_with_session_key() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        let session = sso_session_info();
        let expected_signer = session.sso.as_ref().unwrap().ss_public_key;
        host.session_state().set_session(session);
        let cx = CallContext::new();
        let request = RemoteStatementStoreCreateProofRequest::V1(
            v01::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("myapp.dot", 0),
                statement: statement(),
            },
        );

        let response =
            futures::executor::block_on(StatementStore::create_proof(&host, &cx, request)).unwrap();

        let RemoteStatementStoreCreateProofResponse::V1(inner) = response;
        let v01::StatementProof::Sr25519 { signer, signature } = inner.proof else {
            panic!("expected sr25519 statement proof");
        };
        assert_eq!(signer, expected_signer);
        assert_ne!(signature, [0; 64]);
    }

    #[test]
    fn statement_store_create_proof_rejects_wrong_product_account() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(sso_session_info());
        let cx = CallContext::new();
        let request = RemoteStatementStoreCreateProofRequest::V1(
            v01::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("other.dot", 0),
                statement: statement(),
            },
        );

        let err = futures::executor::block_on(StatementStore::create_proof(&host, &cx, request))
            .unwrap_err();

        assert!(matches!(
            err,
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::UnknownAccount
            ))
        ));
    }

    #[test]
    fn statement_store_create_proof_authorized_signs_with_session_key() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        let session = sso_session_info();
        let expected_signer = session.sso.as_ref().unwrap().ss_public_key;
        host.session_state().set_session(session);
        let cx = CallContext::new();
        let request = RemoteStatementStoreCreateProofAuthorizedRequest::V1(statement());

        let response = futures::executor::block_on(StatementStore::create_proof_authorized(
            &host, &cx, request,
        ))
        .unwrap();

        let RemoteStatementStoreCreateProofAuthorizedResponse::V1(inner) = response;
        let v01::StatementProof::Sr25519 { signer, .. } = inner.proof else {
            panic!("expected sr25519 statement proof");
        };
        assert_eq!(signer, expected_signer);
    }

    #[test]
    fn statement_store_submit_posts_signed_statement_and_waits_for_ack() {
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![r#"{"jsonrpc":"2.0","id":"submit-1","result":"0xok"}"#.to_string()],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("submit-1".to_string());
        let request = RemoteStatementStoreSubmitRequest::V1(signed_statement([7; 32]));

        futures::executor::block_on(StatementStore::submit(&host, &cx, request)).unwrap();

        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        assert_eq!(sent.len(), 1);
        let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(request["method"], "statement_submit");
        let statement_hex = request["params"][0].as_str().unwrap();
        let statement =
            hex::decode(statement_hex.strip_prefix("0x").unwrap_or(statement_hex)).unwrap();
        assert_eq!(
            crate::host_logic::statement_store::decode_signed_statement(&statement).unwrap(),
            signed_statement([7; 32])
        );
    }

    #[test]
    fn statement_store_subscribe_maps_signed_pages() {
        let signed = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([7; 32]),
        )
        .unwrap();
        let unsigned = vec![crate::host_logic::statement_store::StatementField::Data(
            vec![1],
        )]
        .encode();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("sub-1", "remote-sub"),
                new_statements_frame("remote-sub", vec![unsigned, signed]),
            ],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-1".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ))
        .unwrap();

        let item = futures::executor::block_on(subscription.next()).expect("statement page");

        let RemoteStatementStoreSubscribeItem::V1(inner) = item;
        assert!(inner.is_complete);
        assert_eq!(inner.statements, vec![signed_statement([7; 32])]);
        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(request["method"], "statement_subscribeStatement");
        assert_eq!(
            request["params"][0]["matchAny"][0],
            "0x0707070707070707070707070707070707070707070707070707070707070707"
        );
    }

    /// Pages that arrive before the subscribe ack cannot be attributed to the
    /// subscription and must be dropped, even when they carry the id the ack
    /// will later confirm.
    #[test]
    fn statement_store_subscribe_drops_pages_before_subscribe_ack() {
        let rogue = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([9; 32]),
        )
        .unwrap();
        let signed = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([7; 32]),
        )
        .unwrap();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                new_statements_frame("remote-sub-pre", vec![rogue]),
                subscribe_ack_frame("sub-pre", "remote-sub-pre"),
                new_statements_frame("remote-sub-pre", vec![signed]),
            ],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
        let cx = CallContext::with_request_id("sub-pre".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ))
        .unwrap();

        let item = futures::executor::block_on(subscription.next()).expect("statement page");

        assert_eq!(
            item,
            RemoteStatementStoreSubscribeItem::V1(v01::RemoteStatementStoreSubscribeItem {
                statements: vec![signed_statement([7; 32])],
                is_complete: true,
            })
        );
    }

    #[test]
    fn statement_store_subscribe_unsubscribes_remote_subscription_on_drop() {
        let signed = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([7; 32]),
        )
        .unwrap();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("sub-drop", "remote-sub-drop"),
                new_statements_frame("remote-sub-drop", vec![signed]),
            ],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-drop".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ))
        .unwrap();

        let _ = futures::executor::block_on(subscription.next()).expect("statement page");
        drop(subscription);

        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        assert_eq!(sent.len(), 2);
        let unsubscribe: serde_json::Value = serde_json::from_str(&sent[1]).unwrap();
        assert_eq!(unsubscribe["method"], "statement_unsubscribeStatement");
        assert_eq!(unsubscribe["params"][0], "remote-sub-drop");
    }

    #[test]
    fn statement_store_subscribe_emits_empty_completion_page_after_filtering() {
        let unsigned = vec![crate::host_logic::statement_store::StatementField::Data(
            vec![1],
        )]
        .encode();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("sub-empty-complete", "remote-sub-empty"),
                new_statements_frame("remote-sub-empty", vec![unsigned]),
            ],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
        let cx = CallContext::with_request_id("sub-empty-complete".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ))
        .unwrap();

        let item = futures::executor::block_on(subscription.next()).expect("completion page");

        let RemoteStatementStoreSubscribeItem::V1(inner) = item;
        assert!(inner.is_complete);
        assert!(inner.statements.is_empty());
    }

    #[test]
    fn statement_store_subscribe_rejects_topic_limit_violations() {
        let platform = stub_platform();
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-too-many".to_string());
        let topics = vec![[7; 32]; MAX_MATCH_ANY_TOPICS + 1];

        let err = match futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(topics),
            ),
        )) {
            Ok(_) => panic!("topic limit violation should fail subscription start"),
            Err(err) => err,
        };

        let CallError::Domain(RemoteStatementStoreSubscribeError::V1(reason)) = err else {
            panic!("expected statement-store subscribe domain error");
        };
        assert_eq!(
            reason.reason,
            format!(
                "MatchAny has {} topics, maximum is {}",
                MAX_MATCH_ANY_TOPICS + 1,
                MAX_MATCH_ANY_TOPICS
            )
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
    }

    #[test]
    fn statement_store_subscribe_reports_chain_connect_failure() {
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("chain unavailable"),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-connect-fail".to_string());

        let err = match futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        )) {
            Ok(_) => panic!("chain connect failure should fail subscription start"),
            Err(err) => err,
        };

        let CallError::Domain(RemoteStatementStoreSubscribeError::V1(reason)) = err else {
            panic!("expected statement-store subscribe domain error");
        };
        assert!(
            reason
                .reason
                .contains("statement-store connect failed: GenericError"),
            "unexpected reason: {}",
            reason.reason
        );
        assert!(
            reason.reason.contains("chain unavailable"),
            "unexpected reason: {}",
            reason.reason
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
    }
}
