//! `StatementStore` surface: session-key statement proofs plus submit and
//! subscribe flows over the people-chain statement store.

use core::pin::Pin;
use core::task::{Context, Poll};

use super::statement_store_rpc::{self, StatementStoreRpc};
use super::{ProductRuntimeHost, REMOTE_PERMISSION_DENIED_REASON};
use crate::host_logic::statement_store::{
    MAX_MATCH_ALL_TOPICS, MAX_MATCH_ANY_TOPICS, TopicFilterKind, decode_signed_statement,
    parse_new_statements_result, sign_statement_fields, signed_statement_to_scale,
    statement_fields_from_v01, statement_proof_to_v01,
};

use serde_json::Value;
use subxt_rpcs::client::RpcSubscription;
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

impl StatementStore for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "statement_store.subscribe"))]
    async fn subscribe(
        &self,
        _cx: &CallContext,
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
        let statement_store = self.statement_store_rpc();
        let rpc_client = statement_store
            .client("statement-store")
            .await
            .map_err(|reason| {
                CallError::Domain(RemoteStatementStoreSubscribeError::V1(v01::GenericError {
                    reason,
                }))
            })?;
        let subscription = statement_store_rpc::subscribe(&rpc_client, kind, &topics)
            .await
            .map_err(|err| {
                CallError::Domain(RemoteStatementStoreSubscribeError::V1(v01::GenericError {
                    reason: format!("statement-store subscribe failed: {err}"),
                }))
            })?;
        let Some(remote_subscription_id) = subscription.subscription_id().map(ToString::to_string)
        else {
            return Err(CallError::Domain(RemoteStatementStoreSubscribeError::V1(
                v01::GenericError {
                    reason: "statement-store subscribe returned no subscription id".to_string(),
                },
            )));
        };
        let stream = statement_store_subscription_stream(subscription, remote_subscription_id);
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
        inner.product_account_id = Self::normalize_product_account_id(inner.product_account_id)
            .map_err(|()| {
                CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                    v01::RemoteStatementStoreCreateProofError::UnknownAccount,
                ))
            })?;
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
        _cx: &CallContext,
        request: RemoteStatementStoreSubmitRequest,
    ) -> Result<(), CallError<RemoteStatementStoreSubmitError>> {
        let RemoteStatementStoreSubmitRequest::V1(statement) = request;
        self.require_remote_permission(
            v01::RemotePermission::StatementSubmit,
            RemoteStatementStoreSubmitError::V1(v01::GenericError {
                reason: REMOTE_PERMISSION_DENIED_REASON.to_string(),
            }),
        )
        .await?;
        let statement = signed_statement_to_scale(statement).map_err(|reason| {
            CallError::Domain(RemoteStatementStoreSubmitError::V1(v01::GenericError {
                reason,
            }))
        })?;
        self.statement_store_rpc()
            .submit(statement, "statement-store")
            .await
            .map_err(|reason| {
                CallError::Domain(RemoteStatementStoreSubmitError::V1(v01::GenericError {
                    reason: format!("statement-store submit failed: {reason}"),
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
                let topic_count = topics.len();
                return Err(format!(
                    "MatchAny has {topic_count} topics, maximum is {MAX_MATCH_ANY_TOPICS}"
                ));
            }
            Ok((TopicFilterKind::MatchAny, topics))
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "statement_store.subscription_stream"))]
fn statement_store_subscription_stream(
    subscription: RpcSubscription<Value>,
    remote_subscription_id: String,
) -> impl futures::Stream<Item = RemoteStatementStoreSubscribeItem> + Send {
    StatementStoreSubscriptionStream {
        subscription,
        remote_subscription_id,
        is_complete: false,
    }
}

struct StatementStoreSubscriptionStream {
    subscription: RpcSubscription<Value>,
    remote_subscription_id: String,
    is_complete: bool,
}

impl futures::Stream for StatementStoreSubscriptionStream {
    type Item = RemoteStatementStoreSubscribeItem;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let state = self.get_mut();
        loop {
            let value = match Pin::new(&mut state.subscription).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(value))) => value,
                Poll::Ready(Some(Err(_))) | Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
            };
            let page =
                match parse_new_statements_result(state.remote_subscription_id.clone(), &value) {
                    Ok(page) => page,
                    Err(_) => continue,
                };

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

impl ProductRuntimeHost {
    /// `StatementStoreRpc` bound to this runtime's people chain.
    pub(super) fn statement_store_rpc(&self) -> StatementStoreRpc {
        self.services.statement_store.clone()
    }

    fn create_statement_proof(
        &self,
        statement: v01::Statement,
    ) -> Result<v01::StatementProof, StatementProofFailure> {
        let session = self
            .authority
            .session_state()
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

fn statement_proof_v01_error(
    failure: StatementProofFailure,
) -> v01::RemoteStatementStoreCreateProofError {
    match failure {
        StatementProofFailure::NoSession => v01::RemoteStatementStoreCreateProofError::UnableToSign,
        StatementProofFailure::UnableToSign(_reason) => {
            v01::RemoteStatementStoreCreateProofError::UnableToSign
        }
        StatementProofFailure::InvalidStatement(reason) => {
            v01::RemoteStatementStoreCreateProofError::Unknown { reason }
        }
    }
}

fn statement_proof_error(
    failure: StatementProofFailure,
) -> CallError<RemoteStatementStoreCreateProofError> {
    CallError::Domain(RemoteStatementStoreCreateProofError::V1(
        statement_proof_v01_error(failure),
    ))
}

fn statement_proof_authorized_error(
    failure: StatementProofFailure,
) -> CallError<RemoteStatementStoreCreateProofAuthorizedError> {
    CallError::Domain(RemoteStatementStoreCreateProofAuthorizedError::V1(
        statement_proof_v01_error(failure),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        StubPlatform, account_id, new_statements_frame, runtime_config, signed_statement,
        sso_session_info, statement, stub_platform, subscribe_ack_frame, test_spawner,
    };
    use futures::StreamExt;
    use parity_scale_codec::Encode;
    use std::sync::Arc;

    #[test]
    fn statement_store_create_proof_signs_with_session_key() {
        let host =
            ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        let session = sso_session_info();
        let expected_signer = session.sso.as_ref().unwrap().ss_public_key;
        host.test_session_state().set_session(session);
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
            ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.test_session_state().set_session(sso_session_info());
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
            ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        let session = sso_session_info();
        let expected_signer = session.sso.as_ref().unwrap().ss_public_key;
        host.test_session_state().set_session(session);
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
            rpc_responses: vec![r#"{"jsonrpc":"2.0","id":"truapi:1","result":"0xok"}"#.to_string()],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
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
    fn statement_store_submit_requires_remote_permission_before_rpc() {
        let platform = Arc::new(StubPlatform {
            remote_permission_denied: true,
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("submit-1".to_string());
        let request = RemoteStatementStoreSubmitRequest::V1(signed_statement([7; 32]));

        let err =
            futures::executor::block_on(StatementStore::submit(&host, &cx, request)).unwrap_err();

        match err {
            CallError::Domain(RemoteStatementStoreSubmitError::V1(v01::GenericError {
                reason,
            })) => assert_eq!(reason, REMOTE_PERMISSION_DENIED_REASON),
            other => panic!("expected statement-store permission denial, got {other:?}"),
        }
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
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
                subscribe_ack_frame("truapi:1", "remote-sub"),
                new_statements_frame("remote-sub", vec![unsigned, signed]),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
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

    /// Pages that arrive before the subscribe ack are buffered by remote
    /// subscription id and replayed once the ack confirms the subscription.
    #[test]
    fn statement_store_subscribe_buffers_pages_before_subscribe_ack() {
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
                subscribe_ack_frame("truapi:1", "remote-sub-pre"),
                new_statements_frame("remote-sub-pre", vec![signed]),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
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
                statements: vec![signed_statement([9; 32])],
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
                subscribe_ack_frame("truapi:1", "remote-sub-drop"),
                new_statements_frame("remote-sub-drop", vec![signed]),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
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
                subscribe_ack_frame("truapi:1", "remote-sub-empty"),
                new_statements_frame("remote-sub-empty", vec![unsigned]),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
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
        let host = ProductRuntimeHost::new(
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
        let host = ProductRuntimeHost::new(
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
