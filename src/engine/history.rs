//! `search_history`: read-only keyword search over this conversation.
//!
//! The rolling summary and the segment index are lossy views of old turns.
//! When the user refers to something neither carries, the model can ask for
//! the raw stored turn rows instead of guessing. The search is deterministic
//! (no embeddings, no index file): it scores stored turns by term hits.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use keel_core::{
    ArgValue, ParamKind, ParamSpec, PermissionRequest, RiskLevel, Tool, ToolArgs, ToolCtx,
    ToolError, ToolResult, ToolSchema,
};

use super::store::{StoredSession, StoredTurn};

/// How many turns one call may return.
const MAX_RESULTS: usize = 8;
const DEFAULT_RESULTS: i64 = 5;
const MAX_INPUT_CHARS: usize = 160;
const MAX_ANSWER_CHARS: usize = 400;

pub struct SearchHistoryTool {
    session: Arc<Mutex<StoredSession>>,
}

impl SearchHistoryTool {
    pub fn new(session: Arc<Mutex<StoredSession>>) -> Self {
        Self { session }
    }
}

#[async_trait]
impl Tool for SearchHistoryTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "search_history".to_owned(),
            description: "Search earlier turns of this conversation by keyword. Returns the \
                          matching turns, newest and strongest first, with a bounded input and \
                          answer excerpt and the tools each used. Use it when the user refers to \
                          work from earlier that is no longer in the context."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: vec![
                ParamSpec {
                    name: "query".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "max_results".to_owned(),
                    kind: ParamKind::Integer,
                    required: false,
                },
            ],
        }
    }

    fn permission_requests(&self, _args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        Ok(Vec::new())
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let query = match args.require_str("query") {
            Ok(query) => query.to_owned(),
            Err(error) => return ToolResult::failure(ctx.call_id, "search_history", error),
        };
        let limit = match args.get("max_results") {
            Some(ArgValue::Integer(value)) if *value > 0 => (*value as usize).min(MAX_RESULTS),
            _ => DEFAULT_RESULTS as usize,
        };
        let terms = terms(&query);
        if terms.is_empty() {
            return ToolResult::success(
                ctx.call_id,
                "search_history",
                "the query has no searchable terms",
            );
        }

        let session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut hits: Vec<(u32, usize, String)> = Vec::new();
        for turn in session.turns.iter().rev() {
            let haystack =
                format!("{}\n{}\n{}", turn.input, turn.answer, tool_text(turn)).to_lowercase();
            let score = terms.iter().filter(|term| haystack.contains(*term)).count();
            if score == 0 {
                continue;
            }
            hits.push((turn.seq, score, render(turn, score)));
            if hits.len() >= limit.saturating_mul(4) {
                break;
            }
        }
        if hits.is_empty() {
            return ToolResult::success(
                ctx.call_id,
                "search_history",
                format!("no earlier turn matches `{query}`"),
            );
        }
        hits.sort_by(|left, right| right.1.cmp(&left.1).then(right.0.cmp(&left.0)));
        hits.truncate(limit);

        let mut content = format!("[history search: {query}]\n");
        for (_, _, text) in hits {
            content.push_str(&text);
            content.push('\n');
        }
        ToolResult::success(ctx.call_id, "search_history", content)
    }
}

fn tool_text(turn: &StoredTurn) -> String {
    let mut text = String::new();
    for tool in &turn.tools {
        text.push_str(&tool.name);
        text.push(' ');
        if let Some(args) = &tool.args {
            text.push_str(args);
            text.push(' ');
        }
    }
    text
}

fn render(turn: &StoredTurn, score: usize) -> String {
    let mut text = format!("turn {} · {} term match(es)\n", turn.seq, score);
    text.push_str("user: ");
    text.push_str(&clip(&turn.input, MAX_INPUT_CHARS));
    text.push_str("\nassistant: ");
    text.push_str(&clip(&turn.answer, MAX_ANSWER_CHARS));
    if !turn.tools.is_empty() {
        let names: Vec<&str> = turn
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .take(10)
            .collect();
        text.push_str("\ntools: ");
        text.push_str(&names.join(", "));
    }
    text
}

fn clip(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        return flat;
    }
    let mut clipped: String = flat.chars().take(max_chars).collect();
    clipped.push('…');
    clipped
}

/// Splits a query into searchable terms.
///
/// ASCII words are lowercased; CJK runs are reduced to overlapping bigrams,
/// because a Chinese query has no spaces and an exact long-phrase match would
/// almost never fire. Short runs stay whole.
fn terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for run in query.split(|ch: char| !(ch.is_alphanumeric() || is_cjk(ch))) {
        let run = run.trim().to_lowercase();
        if run.is_empty() {
            continue;
        }
        let cjk = run.chars().all(is_cjk);
        if !cjk {
            if run.chars().count() >= 2 {
                terms.push(run);
            }
            continue;
        }
        let chars: Vec<char> = run.chars().collect();
        if chars.len() <= 2 {
            terms.push(run);
            continue;
        }
        for pair in chars.windows(2) {
            terms.push(pair.iter().collect());
        }
    }
    terms.sort();
    terms.dedup();
    terms
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::{CancelToken, ToolCallId, ToolOutcome, TurnId};
    use std::path::PathBuf;

    fn turn(seq: u32, input: &str, answer: &str) -> StoredTurn {
        StoredTurn {
            seq,
            input: input.to_owned(),
            state: "Done".to_owned(),
            answer: answer.to_owned(),
            thinking: String::new(),
            error: None,
            duration_ms: 1,
            usage: Default::default(),
            verification_passed: None,
            verification: None,
            tools: Vec::new(),
            context: None,
            events: Vec::new(),
        }
    }

    fn session(turns: Vec<StoredTurn>) -> Arc<Mutex<StoredSession>> {
        Arc::new(Mutex::new(StoredSession {
            workspace: PathBuf::from("."),
            rig: "test".to_owned(),
            title: String::new(),
            archived: false,
            snapshot: Default::default(),
            todos: None,
            turns,
        }))
    }

    fn args(query: &str) -> ToolArgs {
        let mut args = ToolArgs::new();
        args.insert("query", ArgValue::String(query.to_owned()));
        args
    }

    fn ctx() -> ToolCtx {
        ToolCtx {
            turn_id: TurnId::new(1),
            call_id: ToolCallId::new(1),
            workspace_root: PathBuf::from("."),
            cancel: CancelToken::new(),
        }
    }

    fn content(result: &ToolResult) -> &str {
        match &result.outcome {
            ToolOutcome::Success { content } => content,
            ToolOutcome::Failure { error } => panic!("tool failed: {error}"),
        }
    }

    #[tokio::test]
    async fn a_query_finds_the_matching_turn() {
        let tool = SearchHistoryTool::new(session(vec![
            turn(1, "fix the parser bug", "patched the parser"),
            turn(2, "write a haiku", "waves at dusk"),
        ]));
        let result = tool.call(args("parser"), &ctx()).await;
        let text = content(&result);
        assert!(text.contains("turn 1"), "{text}");
        assert!(!text.contains("turn 2"), "{text}");
    }

    #[tokio::test]
    async fn a_chinese_query_matches_by_bigrams() {
        let tool = SearchHistoryTool::new(session(vec![turn(
            3,
            "把磁吸改成磁贴",
            "已改成快捷设置磁贴",
        )]));
        let result = tool.call(args("上次说的磁吸是什么"), &ctx()).await;
        assert!(content(&result).contains("turn 3"), "{}", content(&result));
    }

    #[tokio::test]
    async fn no_match_says_so() {
        let tool = SearchHistoryTool::new(session(vec![turn(1, "hello", "hi")]));
        let result = tool.call(args("nonexistent"), &ctx()).await;
        assert!(
            content(&result).contains("no earlier turn"),
            "{}",
            content(&result)
        );
    }
}
