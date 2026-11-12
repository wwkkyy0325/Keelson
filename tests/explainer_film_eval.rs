//! Live: one explainer brief becomes a finished mixed film.
//!
//! The rig writes the claim ledger, the script, the storyboard, the scenes,
//! renders the film and runs the QA gate. This test only prepares the project,
//! sends the brief, nudges past a creative stop if the model waits, and then
//! accepts the result: the claim gate, the sources, the film, the 3D frames,
//! and `kit/qa.py --video`.
//!
//! Run with:
//!   KEELSON_FILM_WORKSPACE=<project> \
//!   cargo test --test explainer_film_eval -- --ignored --nocapture
//!
//! Without the variable the project lives in a temp directory and is deleted.
//! `ANTHROPIC_AUTH_TOKEN` is read by the rig's profile and never printed.

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

const BRIEF: &str = r#"这是一个 80 秒左右的中文论证型混排片项目，题目：

“为什么很多人都在逃避责任：从家庭、学校到大学、企业，一条责任的下坡路”

这是用户的主观论证，不是中立调查。论点是：
1. 家庭把教育责任推给学校和社会；
2. 学校被优绩主义驱动（只看分数、升学率），把成绩的责任推给学生；
3. 大学把本该学校承担的安全与管理责任推给学生：宿舍规则越收越紧（床帘、充电宝、违规电器），却不谈电路和消防设施本身；一些责任书用免责条款先把自己摘出去；
4. 企业把责任推给打工人：不缴社保、逃税避税、工伤不担责；
5. 合起来是一条“下坡路大回环”：责任一层层往下传，最后落到最没有权力的人身上。

立场要求（硬性）：
- 不反驳用户，不写“反方 / 另一方面”来平衡；去收集支持上述论点的真实证据（政策原文、法院判决、官方通报、统计），把它们放进片子。
- 但不得编造：事实必须有真实来源和逐字引文；解释要写明框架；结论标“本片观点”。找不到证据的子命题删掉，不要用“有人说”保留。

交付整条片，不要停在脚本或分镜：
- claims.json 至少 8 条（fact / interpretation / value）；事实满足两个独立发布方或一个一手来源，accessed 写 2026-11-11；每个来源 URL 现在可达。
- script.md 与 storyboard.json：6–8 个场景，75–90 秒；至少 1 个 scene3d 场景，用 3D 表现“责任的下坡回环”这一空间隐喻并标示意；其余场景用 2D 纸面承载证据与引文。每个场景标 asset_type，并引用 claims。
- 写 scenes/*.js；用 kit/timeline.py 量出时长；用 render_film(root) 一次渲染全片；kit/assemble.py 合成；kit/qa.py --video 必须全过。不要手填时长。
- 用户在开工前已批准脚本、分镜、风格、联系表和预览，不需要逐站等待；一口气做到成片，最后汇报。

完成标准：成片存在、qa 全过、contact sheet 你看过、来源可达。最后用中文汇报片长、场景数、claims 数、来源数和 QA 结果。
"#;

const CONTINUE: &str = "继续完成整条片。用户已预授权所有停点；不要停在脚本、分镜、风格、联系表或预览。如果某个 gate 失败，修好它再继续，直到 kit/qa.py --video 通过并给出成片路径。";

fn film_path(root: &Path) -> Option<PathBuf> {
    let board = std::fs::read_to_string(root.join("storyboard.json")).ok()?;
    let board: serde_json::Value = serde_json::from_str(&board).ok()?;
    let output = board["output"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("out/film.mp4"));
    let path = if output.is_absolute() {
        output
    } else {
        root.join(output)
    };
    (path.is_file() && path.metadata().ok()?.len() > 200_000).then_some(path)
}

fn shell(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(args[0])
        .args(&args[1..])
        .current_dir(root)
        .output()
        .expect("the command runs")
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn a_brief_becomes_a_finished_mixed_film() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let kit = repo.join("kits").join("explainer");
    let (root, _temp) = match std::env::var("KEELSON_FILM_WORKSPACE") {
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
    std::fs::copy(
        kit.join("claims.example.json"),
        root.join("claims.example.json"),
    )
    .expect("the claim example");
    std::fs::copy(
        kit.join("storyboard.example.json"),
        root.join("storyboard.example.json"),
    )
    .expect("the storyboard example");

    let rig = Rig {
        name: "explainer".to_owned(),
        title: "Explainer".to_owned(),
        description: String::new(),
        profile: repo.join("profiles/explainer-live.toml"),
        extra_roots: Vec::new(),
        path: repo.join("rigs/explainer.toml"),
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

    let mut outcome = engine
        .run_turn(BRIEF, CancelToken::new())
        .await
        .expect("the turn runs");
    assert_eq!(
        outcome.state,
        TurnState::Done,
        "the film turn did not finish: {:?}",
        outcome.error
    );

    let mut turns = 1;
    while film_path(&root).is_none() && turns < 4 {
        outcome = engine
            .run_turn(CONTINUE, CancelToken::new())
            .await
            .expect("the continuation runs");
        assert_eq!(
            outcome.state,
            TurnState::Done,
            "the continuation did not finish: {:?}",
            outcome.error
        );
        turns += 1;
    }

    // The claim gate is the film's first contract.
    let gate = shell(&root, &["python", "data/check_claims.py"]);
    assert!(
        gate.status.success(),
        "the claim gate failed:\n{}{}",
        String::from_utf8_lossy(&gate.stdout),
        String::from_utf8_lossy(&gate.stderr)
    );
    let ledger: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("claims.json")).expect("claims.json"),
    )
    .expect("claims.json is JSON");
    let claims = ledger["claims"].as_array().expect("claims");
    assert!(claims.len() >= 8, "only {} claims", claims.len());

    let mut urls = Vec::new();
    for claim in claims {
        for source in claim["sources"].as_array().cloned().unwrap_or_default() {
            if let Some(url) = source["url"].as_str() {
                urls.push(url.to_owned());
            }
        }
    }
    urls.sort();
    urls.dedup();
    assert!(urls.len() >= 6, "only {} distinct sources", urls.len());
    let mut dead = Vec::new();
    for url in urls.iter().take(24) {
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
    assert!(dead.is_empty(), "dead sources:\n{}", dead.join("\n"));

    // The storyboard must declare at least one 3D shot, and it must have frames.
    let board: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("storyboard.json")).expect("storyboard.json"),
    )
    .expect("storyboard.json is JSON");
    let scenes = board["scenes"].as_array().expect("scenes");
    assert!(
        (6..=8).contains(&scenes.len()),
        "expected 6-8 scenes, got {}",
        scenes.len()
    );
    let three_d: Vec<String> = scenes
        .iter()
        .filter(|scene| scene["asset_type"] == "scene3d")
        .filter_map(|scene| scene["id"].as_str().map(str::to_owned))
        .collect();
    assert!(!three_d.is_empty(), "no scene3d in the storyboard");
    for id in &three_d {
        let frames = std::fs::read_dir(root.join("out/frames").join(id))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| {
                        entry.path().extension().and_then(|e| e.to_str()) == Some("png")
                    })
                    .count()
            })
            .unwrap_or(0);
        assert!(frames > 0, "scene3d {id} has no frames");
    }

    // The film and its QA gate.
    let film = film_path(&root).expect("the film exists");
    let qa = shell(&root, &["python", "kit/qa.py", "--video"]);
    assert!(
        qa.status.success(),
        "qa.py failed:\n{}{}",
        String::from_utf8_lossy(&qa.stdout),
        String::from_utf8_lossy(&qa.stderr)
    );
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(&film)
        .output()
        .expect("ffprobe runs");
    let duration: f64 = String::from_utf8_lossy(&probe.stdout)
        .trim()
        .parse()
        .unwrap_or(0.0);
    assert!(
        (60.0..=120.0).contains(&duration),
        "film is {duration:.2}s, expected 60-120s"
    );

    println!(
        "film {}: {duration:.2}s, {} scenes ({} 3D: {}), {} claims, {} sources, {} turn(s)",
        film.display(),
        scenes.len(),
        three_d.len(),
        three_d.join(", "),
        claims.len(),
        urls.len(),
        turns
    );
}
