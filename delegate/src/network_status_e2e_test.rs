//! End-to-end verification (ignored by default - requires a live local
//! Freenet node) that `FreenetBridge::query_node_status` gets a **real
//! answer from the real node**, not a fabricated one.
//!
//! This is the load-bearing check for the network-status indicator: the
//! whole feature is worthless if the peer count is anything other than what
//! the node itself believes. Deliberately does *not* assert
//! `peer_count > 0` - a cold node, or one behind a VPN/firewall, legitimately
//! reports zero, and that state is exactly what the indicator exists to
//! surface. What it asserts instead is that the node **answered at all**:
//! `peer_count` is `Some(_)` and `query_error` is `None`, which is only
//! possible if `NodeQuery::NodeDiagnostics` round-tripped over the same
//! `?encodingProtocol=native` WebSocket the contract operations use.
//!
//! Run with:
//! `cargo test network_status_e2e -- --ignored --nocapture`
//! (needs a real `freenet` node on `ws://127.0.0.1:7509`, see CLAUDE.md).

use crate::freenet_bridge::FreenetBridge;

#[tokio::test]
#[ignore = "requires a live local Freenet node (ws://127.0.0.1:7509) - see CLAUDE.md"]
async fn node_status_query_gets_a_real_answer_from_the_real_node() {
    let freenet = FreenetBridge::connect_local()
        .await
        .expect("connect to local Freenet node");

    let status = freenet.query_node_status().await;
    println!("node status: {status:#?}");

    assert!(
        status.query_error.is_none(),
        "the node failed to answer a diagnostics query: {:?}",
        status.query_error
    );
    let peers = status
        .peer_count
        .expect("a node that answered without error must report a peer count");
    println!("node reports {peers} ring connection(s)");

    // Nothing has been asked of the network yet on this fresh connection, so
    // the delegate-side operational signal must be honestly empty rather than
    // optimistically pre-filled.
    assert!(
        status.last_success_secs_ago.is_none(),
        "a fresh connection cannot have a last-successful-operation time yet"
    );
    assert!(status.last_error.is_none());
}

/// The delegate-side half of the signal: a real contract operation moves
/// `last_success_secs_ago` from `None` to `Some(_)`. Uses a GET for a
/// contract id that (almost certainly) does not exist, because `Ok(None)` -
/// "the node answered, and the answer is not-found" - is deliberately
/// counted as a successful round trip: this signal measures whether the node
/// is answering, not whether a particular contract happens to exist.
#[tokio::test]
#[ignore = "requires a live local Freenet node (ws://127.0.0.1:7509) - see CLAUDE.md"]
async fn a_real_contract_operation_updates_the_operational_health_signal() {
    let freenet = FreenetBridge::connect_local()
        .await
        .expect("connect to local Freenet node");

    assert!(freenet.query_node_status().await.last_success_secs_ago.is_none());

    let missing = freenet_stdlib::prelude::ContractInstanceId::from_base58(
        // A syntactically valid but almost certainly unpublished contract id.
        "11111111111111111111111111111111",
    )
    .expect("parse a well-formed contract instance id");

    match freenet.get_state(missing).await {
        Ok(_) => {
            let status = freenet.query_node_status().await;
            println!("after a real GET: {status:#?}");
            assert!(
                status.last_success_secs_ago.is_some(),
                "a completed GET must record a successful round trip"
            );
            assert!(status.last_error.is_none());
        }
        Err(e) => {
            // The documented gateway flakiness (see CLAUDE.md) can exhaust
            // all retries. That is the *other* branch this signal exists for,
            // so assert it was recorded rather than treating it as a test
            // failure - both outcomes are real network behaviour.
            println!("GET failed after retries (real network flakiness): {e}");
            let status = freenet.query_node_status().await;
            println!("after a failed GET: {status:#?}");
            assert!(
                status.last_error.is_some(),
                "an exhausted-retry failure must be recorded as last_error"
            );
        }
    }
}
