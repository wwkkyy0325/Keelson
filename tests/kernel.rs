//! Conformance probes for the pinned kernel.
//!
//! These drive a real driver with a mock model. They fail when the kernel
//! stops behaving as `docs/kernel.md` describes, which is the first thing to
//! check after a kernel upgrade.

use keel_core::{ArgValue, ErrorCode, ModelError, Session, ToolArgs, SNAPSHOT_VERSION};
use keel_runtime::MockStep;
use keel_testkit::{
    assert_cancelled, assert_done, assert_event_order, assert_event_present, assert_failed,
    text_turn_events, tool_turn_events, Probe,
};

fn echo_args() -> ToolArgs {
    let mut args = ToolArgs::new();
    args.insert("text", ArgValue::String("hello".to_owned()));
    args
}

#[tokio::test]
async fn a_text_turn_follows_the_event_contract() {
    let probe = Probe::new(vec![MockStep::Text("hello".to_owned())]).expect("a probe");
    let outcome = probe.run("hi").await;

    assert_done(&outcome).expect("done");
    assert_event_order(probe.events(), &text_turn_events()).expect("event order");
}

#[tokio::test]
async fn a_tool_turn_dispatches_and_audits_the_call() {
    let probe = Probe::new(vec![
        MockStep::ToolCall {
            name: "echo".to_owned(),
            args: echo_args(),
        },
        MockStep::Text("done".to_owned()),
    ])
    .expect("a probe");

    let outcome = probe.run("echo hello").await;

    assert_done(&outcome).expect("done");
    assert_event_order(probe.events(), &tool_turn_events()).expect("event order");
    assert_event_present(probe.events(), "tool_finished").expect("tool_finished");
}

#[tokio::test]
async fn a_model_failure_is_audited() {
    let probe = Probe::new(vec![MockStep::Error(ModelError::provider("boom"))]).expect("a probe");
    let outcome = probe.run("hi").await;

    assert_failed(&outcome, ErrorCode::ModelProvider).expect("a provider failure");
    assert_event_present(probe.events(), "model_failed").expect("model_failed");
}

#[tokio::test]
async fn a_cancelled_turn_is_terminal() {
    let probe = Probe::new(vec![MockStep::Hang]).expect("a probe");
    let outcome = probe.run_cancelled("hi").await;

    assert_cancelled(&outcome).expect("cancelled");
    assert_event_present(probe.events(), "turn_cancelled").expect("turn_cancelled");
}

#[tokio::test]
async fn the_snapshot_version_is_enforced() {
    let probe = Probe::new(vec![MockStep::Text("hello".to_owned())]).expect("a probe");
    assert_done(&probe.run("hi").await).expect("done");

    let snapshot = probe.snapshot();
    assert_eq!(snapshot.format_version, SNAPSHOT_VERSION);

    let mut newer = snapshot;
    newer.format_version = SNAPSHOT_VERSION + 1;
    assert!(
        Session::restore(newer).is_err(),
        "a newer snapshot must be refused, not guessed"
    );
}
