//! Live: one explainer turn from a topic to the first two gates.
//!
//! The provider and the key come from the rig's profile, exactly as the
//! desktop uses them. The turn is asked to stop after the script and the
//! storyboard; the test then runs the claim gate and checks that the source
//! URLs resolve, so a plausible-looking but invented citation fails.
//!
//! Run with:
//!   cargo test --test explainer_eval -- --ignored --nocapture
//!
//! `KEELSON_EXPLAINER_WORKSPACE` keeps the project for inspection instead of
//! using a temporary directory.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;
use keel_core::{
    CancelToken, GrantScope, InteractionKind, InteractionPort, InteractionRequest,
    InteractionResponse, TurnState,
};
use keelson::engine::{Engine, ModelOverride, OpenRequest};
use keelson::rig::Rig;

struct AutoApprove;

#[async_trait]
impl InteractionPort for AutoApprove {
    async fn request(
        &self,
        request: InteractionRequest,
        _cancel: &CancelToken,
    ) -> InteractionResponse {
        match request.kind {
            InteractionKind::PermissionApproval => InteractionResponse::Approved {
                scope: GrantScope::Once,
            },
            _ => InteractionResponse::Selected {
                option: request
                    .options
                    .first()
                    .map(|option| option.id.clone())
                    .unwrap_or_else(|| "ok".to_owned()),
            },
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the target dir");
    for entry in std::fs::read_dir(from).expect("a readable dir").flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else if !target.exists() {
            std::fs::copy(entry.path(), target).expect("a copied file");
        }
    }
}

const PROMPT: &str = r#"这是一个 90 秒的中文论证型解释片项目，题目：

“为什么很多人觉得整个社会都在‘去责任化’，越来越不愿意承担责任？”

只做前两个停点。不要渲染、不要写场景代码、不要运行 shell。

1. 先检索并核实，写 claims.json。每条 claim 包含：
   - id、type（fact / interpretation / value）、statement、scenes、status；
   - fact 至少两个不同发布方，或一个一手来源。每条来源必须有 title、
     publisher、url、kind（primary / secondary）、accessed（写成 2026-11-11）、
     quote（逐字引文，不要改写或翻译）；
   - interpretation 必须有 basis（借用的框架或作者）；
   - value 必须有 label（例如“本片观点”）；
   - 来源之间有冲突时，status 写 conflicted，并用 conflict 说明分歧。
   不要编造来源、数字、机构或引文；找不到证据的子命题就删掉，不要用
   “有人说”保留它。

2. 写 script.md：分场景旁白，90 秒左右，结构是现象 → 解释 → 反方 →
   结论。“去责任化”本身是论点，不是事实：把它拆成可验证的子命题，
   结论明确标为本片观点。

3. 写 storyboard.json：4 到 6 个场景，每个场景有 id、kicker、headline、
   narration、visual、motion、claims（引用 claims.json 的 id）。

写完后逐条核对：每个 fact 是否满足来源规则，每个场景引用的 claim 是否存在。
"#;

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn a_topic_becomes_an_evidence_dossier_and_a_storyboard() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let kit = repo.join("kits").join("explainer");
    let (root, _temp) = match std::env::var("KEELSON_EXPLAINER_WORKSPACE") {
        Ok(path) => (PathBuf::from(path), None),
        Err(_) => {
            let temp = tempfile::TempDir::new().expect("a temp dir");
            let path = temp.path().join("film");
            (path, Some(temp))
        }
    };
    std::fs::create_dir_all(&root).expect("the project dir");
    copy_dir(&kit.join("kit"), &root.join("kit"));
    copy_dir(&kit.join("assets"), &root.join("assets"));
    std::fs::create_dir_all(root.join("data")).expect("the data dir");
    std::fs::copy(
        kit.join("data/check_claims.py"),
        root.join("data/check_claims.py"),
    )
    .expect("the claim checker");
    if !root.join("claims.example.json").exists() {
        std::fs::copy(
            kit.join("claims.example.json"),
            root.join("claims.example.json"),
        )
        .expect("the claim example");
    }

    let rig = Rig {
        name: "explainer".to_owned(),
        title: "Explainer".to_owned(),
        description: String::new(),
        profile: repo.join("archive/rigs-v1/explainer-live.toml"),
        extra_roots: Vec::new(),
        path: repo.join("archive/rigs-v1/explainer.toml"),
    };
    // An optional override pins a different model. Since kernel 0.2.15 the
    // rig's own `deepseek-flash` also survives a thinking-mode tool follow-up,
    // so the default path is the normal one.
    let model = std::env::var("KEELSON_EVAL_MODEL")
        .ok()
        .map(|name| ModelOverride {
            provider: "anthropic-compatible".to_owned(),
            model: name,
            base_url: Some("https://api.deepseek.com/anthropic".to_owned()),
            api_key: std::env::var("ANTHROPIC_AUTH_TOKEN").expect("the provider key"),
            context_window: None,
        });
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: root.clone(),
        state_dir: root.join(".state"),
        store_path: root.join(".state/session.json"),
        interactions: Arc::new(AutoApprove),
        deltas: None,
        model,
        permission: None,
        acceptance: false,
        search_url: None,
        search_key: None,
    })
    .expect("the engine opens");

    let outcome = engine
        .run_turn(PROMPT, CancelToken::new())
        .await
        .expect("the turn runs");
    assert_eq!(
        outcome.state,
        TurnState::Done,
        "the turn did not finish: {:?}",
        outcome.error
    );

    let ledger: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("claims.json")).expect("claims.json was written"),
    )
    .expect("claims.json is JSON");
    let claims = ledger["claims"].as_array().expect("claims is an array");
    assert!(claims.len() >= 5, "only {} claims", claims.len());

    let mut facts = 0;
    let mut interpretations = 0;
    let mut values = 0;
    let mut urls = Vec::new();
    for claim in claims {
        match claim["type"].as_str() {
            Some("fact") => facts += 1,
            Some("interpretation") => interpretations += 1,
            Some("value") => values += 1,
            other => panic!("unknown claim type {other:?}"),
        }
        for source in claim["sources"].as_array().cloned().unwrap_or_default() {
            if let Some(url) = source["url"].as_str() {
                urls.push(url.to_owned());
            }
        }
    }
    assert!(facts >= 1, "no factual claim");
    assert!(interpretations >= 1, "no interpretation");
    urls.sort();
    urls.dedup();

    let gate = Command::new("python")
        .arg("data/check_claims.py")
        .current_dir(&root)
        .output()
        .expect("python runs the claim gate");
    assert!(
        gate.status.success(),
        "the claim gate failed:\n{}{}",
        String::from_utf8_lossy(&gate.stdout),
        String::from_utf8_lossy(&gate.stderr)
    );

    let storyboard: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("storyboard.json")).expect("storyboard.json"),
    )
    .expect("storyboard.json is JSON");
    let scenes = storyboard["scenes"].as_array().expect("scenes");
    assert!(
        (4..=6).contains(&scenes.len()),
        "expected 4-6 scenes, got {}",
        scenes.len()
    );
    let with_claims = scenes
        .iter()
        .filter(|scene| {
            scene["claims"]
                .as_array()
                .map(|claims| !claims.is_empty())
                .unwrap_or(false)
        })
        .count();
    assert!(
        with_claims >= 3,
        "only {with_claims} scenes name their claims"
    );
    assert!(root.join("script.md").is_file(), "script.md is missing");

    // A plausible citation that does not resolve is still a bad source.
    let mut unreachable = Vec::new();
    for url in urls.iter().take(16) {
        let output = Command::new("curl")
            .args([
                "-s",
                "-L",
                "--max-time",
                "15",
                "-A",
                "Mozilla/5.0",
                "-o",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
                "-w",
                "%{http_code}",
                url,
            ])
            .output();
        let code = output
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .unwrap_or_else(|_| "000".to_owned());
        println!("source {url} -> {code}");
        if code == "404" {
            unreachable.push(format!("{url} -> 404"));
        }
    }
    assert!(
        unreachable.is_empty(),
        "cited sources that do not exist:\n{}",
        unreachable.join("\n")
    );

    println!(
        "project {}: {} claims ({facts} fact, {interpretations} interpretation, {values} value), \
         {} scenes, {} scenes with claims, {} distinct source URLs",
        root.display(),
        claims.len(),
        scenes.len(),
        with_claims,
        urls.len()
    );
    let _ = &_temp;
}

/// A second turn on an existing project: replace or remove a source whose URL
/// no longer resolves, then re-check every citation.
///
/// Run with:
///   KEELSON_EXPLAINER_WORKSPACE=<the project> \
///   KEELSON_EVAL_MODEL=deepseek-chat \
///   cargo test --test explainer_eval -- --ignored --nocapture
#[tokio::test]
#[ignore = "needs a live provider key and an existing project"]
async fn a_dead_source_is_replaced_or_removed() {
    let Ok(workspace) = std::env::var("KEELSON_EXPLAINER_WORKSPACE") else {
        eprintln!("skipping: set KEELSON_EXPLAINER_WORKSPACE to an existing project");
        return;
    };
    let root = PathBuf::from(workspace);
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rig = Rig {
        name: "explainer".to_owned(),
        title: "Explainer".to_owned(),
        description: String::new(),
        profile: repo.join("archive/rigs-v1/explainer-live.toml"),
        extra_roots: Vec::new(),
        path: repo.join("archive/rigs-v1/explainer.toml"),
    };
    let model = std::env::var("KEELSON_EVAL_MODEL")
        .ok()
        .map(|name| ModelOverride {
            provider: "anthropic-compatible".to_owned(),
            model: name,
            base_url: Some("https://api.deepseek.com/anthropic".to_owned()),
            api_key: std::env::var("ANTHROPIC_AUTH_TOKEN").expect("the provider key"),
            context_window: None,
        });
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: root.clone(),
        state_dir: root.join(".state"),
        store_path: root.join(".state/session.json"),
        interactions: Arc::new(AutoApprove),
        deltas: None,
        model,
        permission: None,
        acceptance: false,
        search_url: None,
        search_key: None,
    })
    .expect("the engine opens");

    let prompt = r#"来源可达性检查发现：claims.json 里 c11 的 sociology.cssn.cn PDF
（http://sociology.cssn.cn/webpic/web/sociology/upload/2013/12/d20131227210454305.pdf）
现在返回 404。

请处理这一处：
- 用 web_search / web_fetch 找一页现在可访问、能支持同一判断的来源，替换它
  （保留逐字引文和访问日期）；或者，因为 c11 是 interpretation 且已有 basis，
  直接删除这条来源。
- 只改 c11 的这一处，不要改动其他 claim、场景或脚本。
- 更新 claims.json 后，用 web_fetch 打开新链接确认可访问。
用一句话说明你做了什么。
"#;
    let outcome = engine
        .run_turn(prompt, CancelToken::new())
        .await
        .expect("the turn runs");
    assert_eq!(
        outcome.state,
        TurnState::Done,
        "the repair turn did not finish: {:?}",
        outcome.error
    );

    let gate = Command::new("python")
        .arg("data/check_claims.py")
        .current_dir(&root)
        .output()
        .expect("python runs the claim gate");
    assert!(
        gate.status.success(),
        "the claim gate failed after the repair:\n{}{}",
        String::from_utf8_lossy(&gate.stdout),
        String::from_utf8_lossy(&gate.stderr)
    );

    let ledger: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("claims.json")).expect("claims.json"),
    )
    .expect("claims.json is JSON");
    let mut urls = Vec::new();
    for claim in ledger["claims"].as_array().expect("claims") {
        for source in claim["sources"].as_array().cloned().unwrap_or_default() {
            if let Some(url) = source["url"].as_str() {
                urls.push(url.to_owned());
            }
        }
    }
    urls.sort();
    urls.dedup();
    let mut dead = Vec::new();
    for url in &urls {
        let code = Command::new("curl")
            .args([
                "-s",
                "-L",
                "--max-time",
                "15",
                "-A",
                "Mozilla/5.0",
                "-o",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
                "-w",
                "%{http_code}",
                url,
            ])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .unwrap_or_else(|_| "000".to_owned());
        println!("source {url} -> {code}");
        if code == "404" {
            dead.push(format!("{url} -> 404"));
        }
    }
    assert!(dead.is_empty(), "still dead:\n{}", dead.join("\n"));
    println!("repair: {} sources checked, none 404", urls.len());
}
