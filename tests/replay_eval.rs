//! Live replay of a recorded Claude Code transcript through the real engine.
//!
//! The test copies a project's tracked tree into a temp workspace, extracts
//! the human prompts from a Claude Code JSONL transcript (the same filter the
//! host uses: `type == user`, `origin.kind == human`, not a sidechain),
//! rewrites absolute paths that point at the original project, and runs one
//! engine turn per prompt against a live model. After every turn it reads the
//! kernel's own events (`context_pressure`, `history_assembled`,
//! `context_escalated`) and appends a row to `turns.jsonl`, so the report is
//! the engine's view, not a re-estimate.
//!
//! ```text
//! REPLAY_TRANSCRIPT=... REPLAY_WORKSPACE_SOURCE=... \
//! REPLAY_START=105 REPLAY_COUNT=80 REPLAY_BUDGET_S=6600 \
//! KEELSON_EVAL_BASE_URL=http://... KEELSON_EVAL_MODEL=q2_0 \
//! cargo test --test replay_eval -- --ignored --nocapture
//! ```
//!
//! Limits are deliberate: a turn that does not finish within
//! `REPLAY_TURN_TIMEOUT_S` is cancelled and the next prompt runs; the loop
//! stops at `REPLAY_BUDGET_S`; three model errors in a row end the run.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use keel_core::{
    CancelToken, Event, GrantScope, InteractionKind, InteractionPort, InteractionRequest,
    InteractionResponse,
};
use keelson::engine::{Engine, OpenRequest};
use keelson::rig::Rig;
use serde::Serialize;
use serde_json::Value;

mod common;

/// Answers every interaction instead of deferring it. The replay measures the
/// conversation, so a deferred choice must not leave the segment with an
/// open item that the summary is then forbidden to absorb.
struct AutoAnswerPort;

#[async_trait]
impl InteractionPort for AutoAnswerPort {
    async fn request(
        &self,
        request: InteractionRequest,
        _cancel: &CancelToken,
    ) -> InteractionResponse {
        if let Some(recommended) = request.recommended {
            return InteractionResponse::Selected {
                option: recommended,
            };
        }
        if let Some(option) = request.options.first() {
            return InteractionResponse::Selected {
                option: option.id.clone(),
            };
        }
        match request.kind {
            InteractionKind::PermissionApproval | InteractionKind::PlanApproval => {
                InteractionResponse::Approved {
                    scope: GrantScope::Once,
                }
            }
            _ => InteractionResponse::Freeform {
                text: "proceed".to_owned(),
            },
        }
    }
}

#[derive(Serialize)]
struct EscalationRow {
    from: u32,
    to: u32,
    reason: String,
}

#[derive(Serialize)]
struct TurnRow {
    index: usize,
    prompt_chars: usize,
    wall_ms: u64,
    state: String,
    error: Option<String>,
    answer_chars: usize,
    model_calls: u32,
    tool_calls: u32,
    events: u32,
    event_bytes: u64,
    input_tokens: u32,
    output_tokens: u32,
    cached_input_tokens: u32,
    context_used: Option<u32>,
    context_ceiling: Option<u32>,
    pressure: Option<String>,
    detail_turns: Option<u32>,
    digest_turns: Option<u32>,
    dropped_turns: Option<u32>,
    assembled_tokens: Option<u32>,
    record_segments: Option<u32>,
    record_tokens: Option<u32>,
    summary_tokens: Option<u32>,
    recall_segments: Option<u32>,
    recall_tokens: Option<u32>,
    /// Summary rewrites in this turn (a seal may defer one).
    summary_updates: u32,
    summary_chars: Option<u32>,
    /// Segments this rewrite absorbed. The session merges these cumulatively;
    /// the event carries the per-rewrite list, not the running total.
    summary_covers: Option<u32>,
    summary_failed: u32,
    /// Tool results elided by the kernel's overflow path.
    elided_messages: u32,
    escalations: Vec<EscalationRow>,
    budget_in_force: u32,
    store_bytes: u64,
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

/// The human prompts of a Claude Code transcript, oldest first.
fn extract_prompts(path: &Path) -> Vec<String> {
    let file = std::fs::File::open(path).expect("open the transcript");
    let reader = BufReader::new(file);
    let mut prompts = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("user") {
            continue;
        }
        if value
            .get("isSidechain")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        if value.pointer("/origin/kind").and_then(Value::as_str) != Some("human") {
            continue;
        }
        let Some(content) = value.pointer("/message/content") else {
            continue;
        };
        let text = match content {
            Value::String(text) => text.clone(),
            Value::Array(blocks) => blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => continue,
        };
        if !text.trim().is_empty() {
            prompts.push(text);
        }
    }
    prompts
}

/// Copies the tracked tree at HEAD. A full directory copy of a real project
/// is too large; a shared clone checks out the tracked tree and shares the
/// object store with the source, so nothing in the source changes.
fn copy_head(source: &Path, dest: &Path) {
    let status = Command::new("git")
        .args(["clone", "--quiet", "--shared"])
        .arg(source)
        .arg(dest)
        .status()
        .expect("run git clone");
    assert!(status.success(), "git clone failed for {source:?}");
}

/// Case-insensitive replacement for ASCII needles. The transcript mixes
/// `D:\...` and `d:\...`; both must find the copied workspace.
fn replace_ci(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_owned();
    }
    let hay = haystack.as_bytes();
    let needle = needle.as_bytes();
    let mut out = String::with_capacity(haystack.len());
    let mut i = 0;
    while i < hay.len() {
        if i + needle.len() <= hay.len() && hay[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            out.push_str(replacement);
            i += needle.len();
        } else {
            let ch = haystack[i..].chars().next().expect("a character");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn p50_p90(durations: &mut [u64]) -> (u64, u64) {
    if durations.is_empty() {
        return (0, 0);
    }
    durations.sort_unstable();
    let pick = |percent: usize| {
        let index = (durations.len() - 1) * percent / 100;
        durations[index]
    };
    (pick(50), pick(90))
}

#[tokio::test]
#[ignore = "replays a long transcript against a live model"]
async fn a_recorded_transcript_replays_through_the_engine() {
    let Some(model) = common::eval_model() else {
        eprintln!("skipping: set DEEPSEEK_API_KEY or KEELSON_EVAL_BASE_URL");
        return;
    };
    let transcript = PathBuf::from(
        std::env::var("REPLAY_TRANSCRIPT").expect("REPLAY_TRANSCRIPT must point at a transcript"),
    );
    let source = PathBuf::from(
        std::env::var("REPLAY_WORKSPACE_SOURCE")
            .expect("REPLAY_WORKSPACE_SOURCE must point at a project"),
    );
    let start = env_usize("REPLAY_START", 1).max(1);
    let count = env_usize("REPLAY_COUNT", 40).max(1);
    let budget_s = env_u64("REPLAY_BUDGET_S", 6600);
    let turn_timeout_s = env_u64("REPLAY_TURN_TIMEOUT_S", 300);
    let out_dir = PathBuf::from(
        std::env::var("REPLAY_OUT").unwrap_or_else(|_| "target/replay-report".to_owned()),
    );

    let prompts = extract_prompts(&transcript);
    eprintln!(
        "transcript has {} human prompts; replaying {}..{}",
        prompts.len(),
        start,
        start + count - 1
    );

    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("workspace");
    copy_head(&source, &workspace);
    let state_dir = temp.path().join("state");
    std::fs::create_dir_all(&state_dir).expect("create the state dir");
    let store_path = state_dir.join("session.json");
    let source_text = source.to_string_lossy().to_string();
    let workspace_text = workspace.to_string_lossy().to_string();
    eprintln!("workspace: {workspace_text}");

    std::fs::create_dir_all(&out_dir).expect("create the report dir");
    let mut turns_file = std::fs::File::create(out_dir.join("turns.jsonl")).expect("create turns");
    let mut rows: Vec<TurnRow> = Vec::new();

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rig = Rig {
        name: "replay".to_owned(),
        title: "Replay".to_owned(),
        description: "Replay a recorded transcript.".to_owned(),
        profile: root.join("profiles/replay.toml"),
        extra_roots: Vec::new(),
        path: root.join("rigs/replay.toml"),
    };
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: workspace.clone(),
        state_dir,
        store_path: store_path.clone(),
        interactions: Arc::new(AutoAnswerPort),
        deltas: None,
        model: Some(model),
        permission: None,
        acceptance: false,
        search_url: None,
        search_key: None,
    })
    .expect("the engine opens");

    let started = Instant::now();
    let mut failures = 0u32;
    let selected: Vec<(usize, String)> = prompts
        .iter()
        .enumerate()
        .skip(start - 1)
        .take(count)
        .map(|(offset, prompt)| {
            (
                offset + 1,
                replace_ci(prompt, &source_text, &workspace_text),
            )
        })
        .collect();
    let total = selected.len();

    for (done, (index, prompt)) in selected.into_iter().enumerate() {
        let elapsed = started.elapsed();
        if elapsed >= Duration::from_secs(budget_s) {
            eprintln!(
                "budget reached after {done} turns ({:.0} s)",
                elapsed.as_secs_f64()
            );
            break;
        }
        let remaining = budget_s.saturating_sub(elapsed.as_secs());
        let timeout = Duration::from_secs(turn_timeout_s.min(remaining.max(1)));

        let cancel = CancelToken::new();
        let watcher = {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(timeout).await;
                cancel.cancel();
            })
        };
        let turn_started = Instant::now();
        let result = engine.run_turn(&prompt, cancel).await;
        watcher.abort();
        let wall_ms = u64::try_from(turn_started.elapsed().as_millis()).unwrap_or(u64::MAX);

        let mut tool_calls = 0u32;
        let mut pressure = None;
        let mut assembled = None;
        let mut escalations = Vec::new();
        let mut event_bytes = 0u64;
        let mut summary_updates = 0u32;
        let mut summary_chars = None;
        let mut summary_covers = None;
        let mut summary_failed = 0u32;
        let mut elided_messages = 0u32;
        let records = engine.events().snapshot();
        for record in &records {
            event_bytes = event_bytes.saturating_add(
                serde_json::to_string(&record.event).map_or(0, |json| json.len()) as u64,
            );
            match &record.event {
                Event::ToolStarted { .. } => tool_calls = tool_calls.saturating_add(1),
                Event::ContextPressure {
                    used,
                    ceiling,
                    level,
                    ..
                } => {
                    pressure = Some((*used, *ceiling, level.clone()));
                }
                Event::HistoryAssembled {
                    detail_turns,
                    digest_turns,
                    dropped_turns,
                    tokens,
                    record_segments,
                    record_tokens,
                    summary_tokens,
                    recall_segments,
                    recall_tokens,
                    ..
                } => {
                    assembled = Some((
                        *detail_turns,
                        *digest_turns,
                        *dropped_turns,
                        *tokens,
                        *record_segments,
                        *record_tokens,
                        *summary_tokens,
                        *recall_segments,
                        *recall_tokens,
                    ));
                }
                Event::SummaryUpdated {
                    chars,
                    fallback,
                    covers,
                    ..
                } => {
                    if !*fallback {
                        summary_updates = summary_updates.saturating_add(1);
                        summary_chars = Some(*chars);
                        summary_covers = Some(covers.len() as u32);
                    }
                }
                Event::SummaryFailed { .. } => {
                    summary_failed = summary_failed.saturating_add(1);
                }
                Event::ContextElided { messages, .. } => {
                    elided_messages = elided_messages.saturating_add(*messages);
                }
                Event::ContextEscalated {
                    from, to, reason, ..
                } => {
                    escalations.push(EscalationRow {
                        from: *from,
                        to: *to,
                        reason: reason.clone(),
                    });
                }
                _ => {}
            }
        }

        let (state, error, answer_chars, model_calls, usage) = match &result {
            Ok(outcome) => (
                outcome.state.to_string(),
                outcome.error.as_ref().map(|error| format!("{error}")),
                outcome.answer.body.len(),
                outcome.model_calls,
                outcome.usage,
            ),
            Err(error) => (
                "error".to_owned(),
                Some(format!("{error}")),
                0,
                0,
                keel_core::TokenUsage::default(),
            ),
        };
        match &result {
            Ok(_) => failures = 0,
            Err(_) => failures = failures.saturating_add(1),
        }

        let store_bytes = std::fs::metadata(&store_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        let row = TurnRow {
            index,
            prompt_chars: prompt.chars().count(),
            wall_ms,
            state: state.clone(),
            error: error.clone(),
            answer_chars,
            model_calls,
            tool_calls,
            events: records.len() as u32,
            event_bytes,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            context_used: pressure.as_ref().map(|point| point.0),
            context_ceiling: pressure.as_ref().map(|point| point.1),
            pressure: pressure.as_ref().map(|point| point.2.clone()),
            detail_turns: assembled.map(|value| value.0),
            digest_turns: assembled.map(|value| value.1),
            dropped_turns: assembled.map(|value| value.2),
            assembled_tokens: assembled.map(|value| value.3),
            record_segments: assembled.map(|value| value.4),
            record_tokens: assembled.map(|value| value.5),
            summary_tokens: assembled.map(|value| value.6),
            recall_segments: assembled.map(|value| value.7),
            recall_tokens: assembled.map(|value| value.8),
            summary_updates,
            summary_chars,
            summary_covers,
            summary_failed,
            elided_messages,
            escalations,
            budget_in_force: engine.context_budget(),
            store_bytes,
        };
        let line = serde_json::to_string(&row).unwrap_or_default();
        let _ = writeln!(turns_file, "{line}");
        let _ = turns_file.flush();

        eprintln!(
            "[{}/{total}] prompt {index}: {:.0} s {state} calls={model_calls} tools={tool_calls} \
             ctx={}/{} {} detail={} digest={} drop={} rec={} sumTok={} recall={} sum={} cov={} el={} in={} out={} \
             esc={} ev={} store={} KB",
            done + 1,
            row.wall_ms as f64 / 1000.0,
            row.context_used.unwrap_or(0),
            row.context_ceiling.unwrap_or(0),
            row.pressure.as_deref().unwrap_or("-"),
            row.detail_turns.unwrap_or(0),
            row.digest_turns.unwrap_or(0),
            row.dropped_turns.unwrap_or(0),
            row.record_segments.unwrap_or(0),
            row.summary_tokens.unwrap_or(0),
            row.recall_segments.unwrap_or(0),
            row.summary_updates,
            row.summary_covers.unwrap_or(0),
            row.elided_messages,
            row.input_tokens,
            row.output_tokens,
            row.escalations.len(),
            row.events,
            row.store_bytes / 1024,
        );
        rows.push(row);

        if failures >= 3 {
            eprintln!("three model errors in a row; stopping");
            break;
        }
    }

    // Keep the session document with the report: the temp dir is dropped when
    // the test ends, and the event stream carries the summary revisions,
    // memos and tool calls that the per-turn rows do not.
    let _ = std::fs::copy(&store_path, out_dir.join("session.json"));

    // Aggregates over the turns that ran.
    let wall_s = started.elapsed().as_secs_f64();
    let input_tokens: u64 = rows.iter().map(|row| u64::from(row.input_tokens)).sum();
    let output_tokens: u64 = rows.iter().map(|row| u64::from(row.output_tokens)).sum();
    let cached_tokens: u64 = rows
        .iter()
        .map(|row| u64::from(row.cached_input_tokens))
        .sum();
    let done = rows
        .iter()
        .filter(|row| row.state.eq_ignore_ascii_case("done"))
        .count();
    // A run_turn error is labeled "error"; a finished turn whose state is
    // Error is labeled "Error". Count both.
    let errors = rows
        .iter()
        .filter(|row| row.state.eq_ignore_ascii_case("error"))
        .count();
    let cancelled = rows
        .iter()
        .filter(|row| row.state.to_lowercase().contains("cancel"))
        .count();
    let mut durations: Vec<u64> = rows.iter().map(|row| row.wall_ms).collect();
    let (p50, p90) = p50_p90(&mut durations);
    let escalations: Vec<&EscalationRow> =
        rows.iter().flat_map(|row| row.escalations.iter()).collect();
    let summary_updates: u32 = rows.iter().map(|row| row.summary_updates).sum();
    let summary_failed: u32 = rows.iter().map(|row| row.summary_failed).sum();
    let max_summary_chars = rows
        .iter()
        .filter_map(|row| row.summary_chars)
        .max()
        .unwrap_or(0);
    let recall_segments: u32 = rows
        .iter()
        .map(|row| row.recall_segments.unwrap_or(0))
        .sum();
    let recall_tokens: u32 = rows.iter().map(|row| row.recall_tokens.unwrap_or(0)).sum();
    let max_summary_covers = rows
        .iter()
        .filter_map(|row| row.summary_covers)
        .max()
        .unwrap_or(0);

    let summary = serde_json::json!({
        "model": std::env::var("KEELSON_EVAL_MODEL").unwrap_or_default(),
        "transcript": transcript.to_string_lossy(),
        "workspace": workspace_text,
        "start": start,
        "requested": total,
        "ran": rows.len(),
        "done": done,
        "errors": errors,
        "cancelled": cancelled,
        "wall_s": wall_s,
        "turn_p50_ms": p50,
        "turn_p90_ms": p90,
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "cached_input_tokens": cached_tokens,
        "final_budget": engine.context_budget(),
        "context_history": engine.context_history(),
        "escalations": escalations,
        "summary_updates": summary_updates,
        "summary_failed": summary_failed,
        "max_summary_chars": max_summary_chars,
        "max_summary_covers": max_summary_covers,
        "recall_segments": recall_segments,
        "recall_tokens": recall_tokens,
    });
    std::fs::write(
        out_dir.join("summary.json"),
        serde_json::to_string_pretty(&summary).unwrap_or_default(),
    )
    .expect("write summary.json");

    let mut markdown = String::new();
    markdown.push_str("# Transcript replay\n\n");
    markdown.push_str(&format!(
        "- model: `{}`\n- transcript: `{}`\n- prompts: {}..{} of {} requested, {} ran\n",
        std::env::var("KEELSON_EVAL_MODEL").unwrap_or_default(),
        transcript.to_string_lossy(),
        start,
        start + total - 1,
        total,
        rows.len()
    ));
    markdown.push_str(&format!(
        "- wall: {wall_s:.0} s; turns p50 {p50} ms / p90 {p90} ms; done {done}, errors {errors}, cancelled {cancelled}\n"
    ));
    markdown.push_str(&format!(
        "- tokens: in {input_tokens}, out {output_tokens}, cached {cached_tokens}; final working set {}\n",
        engine.context_budget()
    ));
    markdown.push_str(&format!(
        "- summaries: {summary_updates} rewritten, {summary_failed} failed, max {max_summary_chars} chars; recall: {recall_segments} segment(s), {recall_tokens} tokens\n\n"
    ));
    markdown.push_str(
        "| # | s | state | calls | tools | in | out | ctx | det | dig | drop | rec | sumTok | recall | sum# | cov | esc | ev KB | store KB |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for row in &rows {
        markdown.push_str(&format!(
            "| {} | {:.0} | {} | {} | {} | {} | {} | {}/{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            row.index,
            row.wall_ms as f64 / 1000.0,
            row.state,
            row.model_calls,
            row.tool_calls,
            row.input_tokens,
            row.output_tokens,
            row.context_used.unwrap_or(0),
            row.context_ceiling.unwrap_or(0),
            row.detail_turns.unwrap_or(0),
            row.digest_turns.unwrap_or(0),
            row.dropped_turns.unwrap_or(0),
            row.record_segments.unwrap_or(0),
            row.summary_tokens.unwrap_or(0),
            row.recall_segments.unwrap_or(0),
            row.summary_updates,
            row.summary_covers.unwrap_or(0),
            row.escalations.len(),
            row.event_bytes / 1024,
            row.store_bytes / 1024,
        ));
    }
    std::fs::write(out_dir.join("summary.md"), markdown).expect("write summary.md");

    eprintln!(
        "report written to {}\nran {} turns in {:.0} s; done {done}, errors {errors}, cancelled {cancelled}; \
         tokens in {input_tokens} / out {output_tokens} / cached {cached_tokens}; summaries {summary_updates} \
         (failed {summary_failed}); recall {recall_segments} segments / {recall_tokens} tokens; final working set {}",
        out_dir.display(),
        rows.len(),
        wall_s,
        engine.context_budget()
    );
}
