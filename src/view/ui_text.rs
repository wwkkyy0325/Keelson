//! User-facing chrome text for the terminal UI.
//!
//! Everything the user reads in the TUI comes from here, in one of two
//! languages. Model-facing strings are deliberately *not* localized: the
//! messages the kernel injects, tool names, paths and commands stay English.
//!
//! The permission sentence itself is built by the kernel (`InteractionRequest`
//! carries one formatted string); this module localizes the host chrome and
//! the known permission option labels. Localizing the sentence needs
//! structured fields on the kernel request.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    /// Explicit `--lang` wins, then the shell locale, then the OS UI language.
    ///
    /// `auto` and an unknown value both mean "detect"; the parser rejects an
    /// invalid explicit value before this runs.
    pub fn resolve(explicit: Option<&str>) -> Self {
        match explicit {
            Some("zh") => return Self::Zh,
            Some("en") => return Self::En,
            _ => {}
        }
        locale_lang().or_else(os_lang).unwrap_or(Self::En)
    }
}

fn locale_lang() -> Option<Lang> {
    for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        let Ok(value) = std::env::var(name) else {
            continue;
        };
        if value.trim().is_empty() {
            continue;
        }
        return Some(if value.to_ascii_lowercase().starts_with("zh") {
            Lang::Zh
        } else {
            Lang::En
        });
    }
    None
}

/// The OS UI language, independent of the shell environment. Windows is the
/// reference host; other platforms rely on the locale variables above.
#[cfg(windows)]
fn os_lang() -> Option<Lang> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // A LANGID's primary language is its low ten bits; 0x04 is Chinese.
    let id = unsafe { GetUserDefaultUILanguage() };
    Some(if id & 0x03ff == 0x04 {
        Lang::Zh
    } else {
        Lang::En
    })
}

#[cfg(not(windows))]
fn os_lang() -> Option<Lang> {
    None
}

#[derive(Debug, Clone, Copy)]
pub struct Text {
    lang: Lang,
}

impl Text {
    pub fn new(lang: Lang) -> Self {
        Self { lang }
    }

    fn pick(&self, zh: &'static str, en: &'static str) -> &'static str {
        match self.lang {
            Lang::Zh => zh,
            Lang::En => en,
        }
    }

    pub fn empty_hint(&self) -> &'static str {
        self.pick(
            "输入任务；输入 /help 查看命令",
            "type a request; /help lists commands",
        )
    }

    pub fn status_ready(&self) -> &'static str {
        self.pick("就绪", "ready")
    }

    pub fn status_working(&self) -> &'static str {
        self.pick("工作中", "working")
    }

    pub fn task_status(&self, open: usize, total: usize) -> String {
        match self.lang {
            Lang::Zh => format!("任务 {open}/{total}"),
            Lang::En => format!("tasks {open}/{total}"),
        }
    }

    pub fn mode_label(&self, mode: &str) -> String {
        match (self.lang, mode) {
            (Lang::Zh, "workspace_auto") => "工作区",
            (Lang::Zh, "full_auto") => "全自动",
            (Lang::En, "workspace_auto") => "Workspace",
            (Lang::En, "full_auto") => "Full auto",
            _ => mode,
        }
        .to_owned()
    }

    pub fn permission_title(&self) -> &'static str {
        self.pick(" 权限 ", " permission ")
    }

    pub fn permission_hint(&self) -> &'static str {
        self.pick(
            " ↑↓ 选择 · Enter 确认 · Esc 稍后 · Shift+Tab 模式",
            " ↑↓ select · Enter confirm · Esc defer · Shift+Tab mode",
        )
    }

    pub fn cancelling(&self) -> &'static str {
        self.pick("正在取消…", "cancelling…")
    }

    pub fn interjection_queued(&self) -> &'static str {
        self.pick(
            "打断已排队，将在下一个安全点送达",
            "interjection queued for the next safe point",
        )
    }

    pub fn interjection_duplicate(&self) -> &'static str {
        self.pick(
            "这条打断已经在队列里",
            "that interjection is already queued",
        )
    }

    pub fn interjection_full(&self) -> &'static str {
        self.pick("打断队列已满", "interjection queue is full")
    }

    pub fn interjection_late(&self, text: &str) -> String {
        match self.lang {
            Lang::Zh => format!("打断未送达：{text}"),
            Lang::En => format!("interjection not delivered: {text}"),
        }
    }

    pub fn secret_notice(&self, kind: &str) -> String {
        match self.lang {
            Lang::Zh => format!("输入里检测到凭据（{kind}）；未拦截，已从日志中脱敏"),
            Lang::En => format!(
                "credential pattern ({kind}) in the input; not blocked, redacted from the log"
            ),
        }
    }

    pub fn summary_tools(&self, count: usize) -> Option<String> {
        match count {
            0 => None,
            1 => Some(self.pick("1 个工具", "1 tool").to_owned()),
            count => Some(match self.lang {
                Lang::Zh => format!("{count} 个工具"),
                Lang::En => format!("{count} tools"),
            }),
        }
    }

    pub fn summary_files(&self, count: usize) -> Option<String> {
        match count {
            0 => None,
            1 => Some(self.pick("1 个文件", "1 file").to_owned()),
            count => Some(match self.lang {
                Lang::Zh => format!("{count} 个文件"),
                Lang::En => format!("{count} files"),
            }),
        }
    }

    pub fn summary_blocked(&self, count: usize) -> Option<String> {
        match count {
            0 => None,
            1 => Some(self.pick("1 次拦截", "1 blocked").to_owned()),
            count => Some(match self.lang {
                Lang::Zh => format!("{count} 次拦截"),
                Lang::En => format!("{count} blocked"),
            }),
        }
    }

    pub fn help(&self) -> &'static str {
        self.pick(
            "按键：Enter 发送 · Shift+Enter 换行 · Ctrl-C 取消/退出 · Ctrl-O 详情 · Shift+Tab 模式 · PgUp/PgDn 滚动\n\
             命令：/help /details /add-dir /clear /cost /roots /cancel /quit",
            "keys: Enter send · Shift+Enter newline · Ctrl-C cancel/quit · Ctrl-O details · Shift+Tab mode · PgUp/PgDn scroll\n\
             commands: /help /details /add-dir /clear /cost /roots /cancel /quit",
        )
    }

    pub fn cost_turn(&self, input: u64, output: u64, cached: u64) -> String {
        match self.lang {
            Lang::Zh => format!("本轮：输入 {input} 输出 {output} 缓存 {cached}"),
            Lang::En => format!("turn: input {input} output {output} cached {cached}"),
        }
    }

    pub fn cost_session(&self, input: u64, output: u64, cached: u64) -> String {
        match self.lang {
            Lang::Zh => format!("会话：输入 {input} 输出 {output} 缓存 {cached}"),
            Lang::En => format!("session: input {input} output {output} cached {cached}"),
        }
    }

    pub fn background_started(&self, id: &str, program: &str) -> String {
        match self.lang {
            Lang::Zh => format!("后台 {id} {program} 已启动"),
            Lang::En => format!("{id} {program} started"),
        }
    }

    pub fn background_finished(
        &self,
        id: &str,
        program: &str,
        exit_code: Option<i32>,
        output_path: &str,
    ) -> String {
        let code = exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "?".to_owned());
        match self.lang {
            Lang::Zh => format!("后台 {id} {program} 结束（退出码 {code}）：{output_path}"),
            Lang::En => format!("background {id} {program} exited {code}: {output_path}"),
        }
    }

    pub fn sub_agent_finished(
        &self,
        role: &str,
        state: &str,
        model_calls: u32,
        tool_calls: u32,
        input_tokens: u32,
        output_tokens: u32,
    ) -> String {
        match self.lang {
            Lang::Zh => format!(
                "子代理 {role} {state}：{model_calls} 次模型调用 / {tool_calls} 次工具调用 / \
                 输入 {input_tokens} 输出 {output_tokens}"
            ),
            Lang::En => format!(
                "sub-agent {role} {state}: {model_calls} calls / {tool_calls} tools / \
                 {input_tokens} in / {output_tokens} out"
            ),
        }
    }

    pub fn no_roots(&self) -> &'static str {
        self.pick("没有额外根目录", "no roots recorded")
    }

    pub fn add_dir_usage(&self) -> &'static str {
        self.pick(
            "用法：/add-dir <路径> [ro|rw]（默认 rw）",
            "usage: /add-dir <path> [ro|rw] (default rw)",
        )
    }

    pub fn add_dir_unavailable(&self) -> &'static str {
        self.pick(
            "当前上下文不能添加目录",
            "adding directories is not available here",
        )
    }

    pub fn add_dir_failed(&self, error: &str) -> String {
        match self.lang {
            Lang::Zh => format!("无法添加目录：{error}"),
            Lang::En => format!("cannot add root: {error}"),
        }
    }

    pub fn root_added(&self, path: &str, access: &str) -> String {
        match self.lang {
            Lang::Zh => format!("已添加作用域：{path} [{access}]"),
            Lang::En => format!("added root: {path} [{access}]"),
        }
    }

    pub fn turn_cancelled(&self) -> &'static str {
        self.pick("本轮已取消", "turn cancelled")
    }

    pub fn turn_failed(&self) -> &'static str {
        self.pick("本轮失败", "turn failed")
    }

    pub fn empty_answer(&self) -> &'static str {
        self.pick(
            "本轮没有文本回答；过程已保留，可按 Ctrl-O 展开",
            "the turn ended without an answer; press Ctrl-O to expand the process",
        )
    }

    pub fn turn_ended(&self, state: &str) -> String {
        match self.lang {
            Lang::Zh => format!("本轮结束：{state}"),
            Lang::En => format!("turn ended: {state}"),
        }
    }

    pub fn continuation_hint(&self, remaining: usize) -> String {
        match self.lang {
            Lang::Zh => format!("还有 {remaining} 个任务未完成；按 Enter 继续"),
            Lang::En => format!("{remaining} task(s) remain; press Enter to continue"),
        }
    }

    /// Label for a known permission option. English keeps the kernel's label
    /// (one source of truth); Chinese translates the known ids. Unknown
    /// options keep the label the kernel supplied.
    pub fn permission_label(&self, id: &str, fallback: &str) -> String {
        if self.lang == Lang::En {
            return fallback.to_owned();
        }
        let localized = match id {
            "deny" => "拒绝",
            "allow_once" => "仅本次允许",
            "allow_scope" => "本会话允许此路径",
            "full_auto" => "允许并切换到全自动",
            _ => return fallback.to_owned(),
        };
        localized.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_language_wins() {
        assert_eq!(Lang::resolve(Some("zh")), Lang::Zh);
        assert_eq!(Lang::resolve(Some("en")), Lang::En);
        assert_eq!(Lang::resolve(Some("auto")), Lang::resolve(None));
    }

    #[test]
    fn chrome_text_has_both_arms() {
        for lang in [Lang::Zh, Lang::En] {
            let text = Text::new(lang);
            assert!(!text.empty_hint().is_empty());
            assert!(text.help().contains("Ctrl-O"));
            assert!(text.help().contains("/cost"));
            assert!(text.cost_turn(1, 2, 3).contains('1'));
            assert!(text.cost_session(4, 5, 6).contains('4'));
            assert!(!text.background_started("bg-1", "python").is_empty());
            assert!(!text
                .background_finished("bg-1", "python", Some(0), "/tmp/out")
                .is_empty());
            assert!(!text
                .sub_agent_finished("reviewer", "Done", 2, 3, 40, 8)
                .is_empty());
            assert!(text.summary_tools(0).is_none());
            assert!(text
                .summary_tools(2)
                .is_some_and(|value| value.contains('2')));
            assert_eq!(text.permission_label("unknown", "kept"), "kept");
        }
        assert_eq!(Text::new(Lang::En).permission_label("deny", "Deny"), "Deny");
        assert_eq!(Text::new(Lang::Zh).permission_label("deny", "Deny"), "拒绝");
        assert_ne!(
            Text::new(Lang::Zh).permission_title(),
            Text::new(Lang::En).permission_title()
        );
        assert_eq!(Text::new(Lang::Zh).mode_label("workspace_auto"), "工作区");
        assert_eq!(Text::new(Lang::Zh).mode_label("full_auto"), "全自动");
        assert_eq!(
            Text::new(Lang::En).mode_label("workspace_auto"),
            "Workspace"
        );
        assert_eq!(Text::new(Lang::En).mode_label("full_auto"), "Full auto");
        assert_eq!(Text::new(Lang::En).mode_label("custom"), "custom");
        assert_eq!(Text::new(Lang::Zh).task_status(2, 5), "任务 2/5");
        assert_eq!(Text::new(Lang::En).task_status(2, 5), "tasks 2/5");
    }
}
