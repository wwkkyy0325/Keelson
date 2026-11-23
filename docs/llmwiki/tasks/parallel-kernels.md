# 并行内核、记录时序与 rig 显示：四项调整

Status: 第 1、3 项与第 2 项的 A 步已实现并提交（ADR 0039 增补、0064/0067 增补、0076），待打包替换后做运行时确认；第 2 项的 B/C/D 步未开始。

四项来自同一条反馈，改造量差三个数量级，所以分开定案、分开合并。

---

## 1. 记录时序：一轮开始的瞬间就该有文档

### 现状（代码事实）

- 步骤日志 `journal.db` 已经处理对了：`records::StepLog::open` 在第一步行之前就写行
  （`commands.rs:1970` 附近，注释 “The row exists before the first step, so the log never
  starts empty”）。
- 任务文档没有：`taskdoc::write_task` 只在 `send_message` 收尾的 `spawn` 里调用一次
  （`commands.rs:2232`），输入是 `result`、`engine.turn(seq)`、checkpoint diff、`git_state`。
  所以**进行中的一轮在 `progress/` 里根本不存在**，索引也没有行。

### 方案

1. 一轮开始、`log_meta` 组装好之后，先写一份 `status: running` 的文档：`at_ms`（即文件名时间）
   和 `turn` 此刻定死，`task:` 字段即身份。
2. 结束时用**同一个身份**重写（不是新写一份）：`status: done | failed`，补齐 model_calls、
   tools、duration_ms、changes、git。文件名不变，索引重建一次。
3. `taskdoc::Entry.ok: bool` 换成三态 `status`，读旧文档时 `ok`/`status` 互相兼容；`## Next`
   的“原样搬运”逻辑不动。
4. 崩溃或强杀会留下 `running` 文档。不在启动时猜结果：`INDEX.md` 里照实显示 `running`，
   下次同一会话继续时由新一轮覆盖自己的身份，别人的 `running` 保留原样（用户可以手改）。
5. 不做：空对话不写文档（没有轮次就没有记录，`new_session` 的行为不变）。

### 需要

ADR 0064 增补一小节（“文档在两处写，身份在开始时确定”）。

### 实现（已完成）

- `taskdoc::Entry.ok: bool` → `Entry.status: Status{Running, Done, Failed}`；`render` 按三态写，
  `running` 时 Changes 段写“回合还在跑，变更在结束时记录”，Git 段前缀“回合开始时：”。
  读旧文档时 `ok` 仍被认作 done/failed，`## Next` 的搬运逻辑没动。
- `send_message` 在 `log_meta` 之后记 `started_ms`，`spawn` 最开头写 `Status::Running`；
  收尾用**同一个 `at_ms`** 重写，所以是一个文件被替换，不是两份文档。
- 测试 `a_turn_has_its_document_from_the_first_moment`：`running` → 索引出现 `| running |` →
  同一回合结束后文件数仍为 1、内容变 `done`。

---

## 2. 一个壳子、多个内核：多项目并行

### 现状（代码事实）

引擎层其实已经能并行：`DesktopState.open` 按 id 存 `Arc<OpenConversation>`，每会话自带
engine / cancel / seq / running，事件按 `SessionApp::new(app, session_id)` 分频道，
`OPEN_LIMIT = 8` 且“在跑的会话永不淘汰”。真正锁死的是**寻址**和**全局配置**：

| 位置 | 问题 |
|---|---|
| `DesktopState.current` | 唯一“当前会话”指针；`current_open()` 是默认寻址 |
| `send_message(input)` / `cancel_turn()` / `session_status()` / `branch_session()` / `undo_last_turn()` / `turn_changes()` / `load_turns()` | 无会话参数，一律打到 `current_open()` |
| `ensure_idle()` | 以 current 为准：别的项目里有一轮在跑，也会挡住 `new_session` / `branch_session` |
| `state.open` | 键只有 session id，不含 workspace |
| 收尾块 | `taskdoc` / `checkpoint` / `sessions::touch` 用 `current.workspace` |
| `DesktopConfig` | model / provider / permission / rig 全局一份；两个项目不能各用一套 |
| `session_io` | 全局一把锁，不同项目的索引写入互相排队 |

### 方案（四步，可分别合并、每步自洽）

**A. 命令身份化**
所有会话命令显式带 `sessionId`（需要 workspace 的带 workspace）；`current` 降级为 UI focus，
只给“新建会话”这类确实没有目标的命令用。`open` 的键改成 `(workspace, id)`。
验收：两个会话各自 `send`，事件不串、取消只停自己那一个。

**B. 前端多会话 store**
`session` 单例 → `conversations: Map<key, {turns, running, status, changes, interaction}>` +
`focus`。事件按 sessionId 路由（`lib/session.ts` 已有 epoch 守卫，扩成 per-key epoch）。
侧栏给非焦点会话显示“在跑”标记；Transcript 只渲染 focus；滚动与重渲染的既有契约
（ADR 0067 / 0068）不变，只是数据源换成 focus 条目。

**C. 每会话配置快照**
打开时把 model / provider / baseUrl / rig / permission 解析进 `OpenConversation`，之后改设置
只影响新会话。rig 已经是这个语义（ADR 0039），把 model 与 permission 补齐；交互卡片带项目名，
因为两张卡片可能同时在等回答。

**D. 并行 UI**
侧栏项目分组 + 每个会话自己的取消按钮 + 全局“N 个会话在跑”入口；`OPEN_LIMIT` 只在空闲会话里
淘汰，在跑的一律豁免（现状已如此，升到 16 并加测试）。

### 非目标

多窗口、共享内核池、跨项目共享 transcript、把 `history` 查询改成一次读多个项目。第一版只保证
“能同时跑、互不串台”。

### 风险

命令签名全改（`commands.rs` 2682 行里大部分命令）、`protocol.ts` 跟着改、前端 store 重写。
建议 A 单独一个 PR，B 一个，C/D 各一个；A 之前先补两条回归测试（两会话并行、跨项目打开）。

### 需要

新 ADR（0076）：一个窗口持有多个会话内核，命令按会话寻址；`current` 不是数据源。

---

## 3. rig 显示“残留”：根因已定位，不是显示 bug

### 根因

`open_inner` 在 rig 解析**之前**有一条早退：

```rust
// Reopening a conversation the app already has open keeps its engine.
if let Some(open) = state.open_of(&session_id) { return Ok(open_view(...)); }
```

`chooseRig` 的路径正好撞上它：`save_rig`（改 `config.rig`）→ `reopen_session(同一 id)` → 早退
→ 引擎还是旧 rig。状态行 `SessionStatus.rig = engine.rig_name()` 忠实显示了**真正在跑的**
那个 rig，而设置里的默认值已经是新选的。也就是说：**空对话里换 rig 从来没生效过**，显示是诚实
的，说的是真的那个 rig；第一轮也会用旧 rig 跑，并被文档记成旧 rig。

### 方案

1. 早退条件收紧：`有 turns 或正在跑 → 保留引擎`；空且空闲 → 用新 rig 重建引擎并替换 map 条目
   （这也让 ADR 0039 第 2 条“换 rig 后以同一 id 重开”真的成立）。
2. 状态行的 rig 只认一个来源：`SessionView.rig`（随 open 一起回来），不再单独取
   `session_status.rig`；或者保留 `session_status` 但给响应加 sessionId/epoch 守卫，乱序返回
   不许覆盖当前会话。两条都做也不冲突，但只做第 2 条是掩盖问题。
3. 验证：空对话选 B → 状态行立刻 B，且第一轮文档 `rig: B`；有轮次的会话改默认值 → 显示不变；
   运行中切走再切回 → 不变。

### 需要

ADR 0039 增补一句（“保留引擎”的例外），或并入 0076。

---

## 4. 流程：先给方案，再动手

这次的问题不在判断，在授权：用户给的是目标，我把它当成“可以顺手改”，于是把调查做成了执行，
既浪费一轮，也把上下文填满了代码细节。改法：

- 结构性改动（schema、命令签名、存储布局、多会话状态、跨模块重构）→ 先出方案 + 一个二选一，
  等确认；不写代码。
- 只读诊断、单文件小修、用户明确说“改/装/删”的动作 → 直接做。
- 方案里写清：现状事实（带文件行号）、改法、非目标、风险、需要哪条 ADR。

---

## 建议顺序

3 → 1 → 2。理由：3 的根因已经定位，改动限定在 `open_inner` 一处加前端一处，能立刻消掉最烦的
症状；1 是 `taskdoc` 一处加 ADR 增补，一天量级；2 是分四个 PR 的结构改造，值得单独排期。

---

## 3 的实现（已完成）

- `open_inner`：早退条件收紧为「有会话文档 **或** 有活在跑」才保留引擎；空且空闲的对话用当前
  `config.rig` 重建引擎，`install_open` 替换 map 里的旧条目。
- 新增 `conversation_busy(&OpenConversation)`：`cancel` / `running` 任一非空，或
  `engine.background_tasks()` 非空，即算有活。
- `SessionStatus` 增加 `session_id` 字段；`session_status` 先取当前 id 再取该 id 的引擎，
  不再可能出现“答案描述的是另一个会话”。
- 前端：状态行的 rig 改为 `$session.rig`（open 回来的 view 是唯一来源），`status.rig` 只作兜底；
  `loadStatus` 丢弃 `sessionId` 或 `epoch` 不匹配的迟到回答。
- `protocol.ts`：`SessionStatus.sessionId`。
- ADR 0039 增补「被选中的 rig 就是跑的 rig」。

验证（实测）：

- 仓库根：`cargo fmt --check`、`cargo test` 通过。
- `desktop/src-tauri`：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、
  `cargo test` → 75 passed。
- `desktop`：`npm test` → 85 passed、`npm run check` → 0 errors 0 warnings、`npm run build` 通过。

### 构建与安装

- `cargo tauri build --bundles app`（`desktop/`）→ `target/release/bundle/macos/Keelson.app`，
  新二进制 md5 `18c92b30…15fcc6`，vite 前端一并嵌进二进制（`dist/assets/index-Vlt1LJcG.js`）。
- 已替换 `/Applications/Keelson.app`（旧版退到 `~/.Trash/Keelson.app-old-20251123`），
  安装后 md5 与新构建一致。
- 正在运行的进程仍是旧代码装载（进程 88122，启动时的旧 inode）；**退出并重开应用**才生效。

未做：运行时点击验证。验证步骤是「新对话 → 选另一个 rig → 状态行立刻变，且第一轮文档
`rig:` 是新值」。

---

## 附：同一段早退造成的第二个缺陷（已修，ADR 0076）

用户报的现象：发出去的消息不显示，或要刷新才显示；刷新后又必须“终止对话”才能进入，否则停在
失败页。根因和 rig 那项是同一个早退——它返回了 view，却没有移动 `state.current`。

- 后果一：切回一个已经打开的对话后，`current` 还停在刚离开的那个。下一次发送按 `current` 走，
  回合跑在用户没看的对话里；事件带的是那个对话的 id，前端 `owned()` 全丢，所以界面什么都不出。
  证据：`s-18e864ab45bf49a0` 的第 3、4 轮输入完全相同（同一句话被迫发了两遍），而用户正在看的
  `s-18e8b20ffa85b980` 在 17:26 才有一条 `Cancelled`——就是失败页要求的“终止”。
- 后果二：`open_inner` 开头的 `ensure_idle` 检查的是 `current`，不是要打开的对话。别处有一轮在跑
  就拒绝打开任何对话，于是刷新后进不来。

修法（已落地）：

- `send_message` / `cancel_turn` / `session_status` 增加 `session_id`，前端传 `$session.sessionId`；
  `null` 只给启动页用（取消挡住打开的那一轮）。
- 新增 `show_conversation()`：`open_inner` 两条路径都用它移动 `current`，保留引擎也要移。
- `open_inner` 去掉 `ensure_idle`：有文档或有活的对话本来就保留引擎，不会被重建，所以没有需要
  拦的情况；空且空闲的才重建（rig 那项）。
- `OpenConversation` 增加 `workspace` / `state_dir`：点名某对话的命令不必再问 `current`。

A 步收尾之后，寻址不再有例外：`turn_changes` / `load_turns` / `background_tasks` /
`stop_background` / `undo_last_turn` / `branch_session` 全部接受 `session_id`，前端一律传
`shownSession()`；后端只有一个解析入口 `conversation_named(&state, session_id)`——点名就按名找，
没点才落到窗口指针。

同时删掉 `ensure_idle`（以 `current` 为准的那把锁），换成按对象判忙：

- `new_session` 不再被别处的回合挡住：新对话自带 id 和引擎，跑着的那个留在 map 里不受影响。
- `branch_session` / `undo_last_turn` 只要求**被操作的那个**对话空闲。
- `delete_session` / `archive_session` 同样只查目标对话；`archive_session` 还顺带修了一个洞——
  以前只有“当前”对话会通知引擎改 `archived`，别的已打开对话归档后引擎仍以为可写。

还没做：`DesktopConfig` 仍全局一份（两个项目不能各用一套模型/权限/rig），`state.open` 的键
仍只有 session id，`session_io` 仍是一把全局锁。这些是 B/C/D 步。

---

## 附二：跟随底部的判据从来没有把“正在跑的回合”算进去（已修，ADR 0067 增补）

用户报“发消息后对话消息还是得生成完才落进去”。根因不在发送、也不在事件，在
`Transcript.svelte` 的 `bottomKey`：它数的是已渲染块、工具行、背景行、已持久化回合数和错误，
而这些在回合开始的一瞬间全是空的——正在跑的那张卡片带着用户自己的输入出现了，但没有任何一个
计数器跟着变（回合数要等落盘才 +1）。于是从底部发一句话，它被插到视口下方，看着像“没发出去”，
直到回合结束、计数变了才跳上来。

改法：判据提成纯函数 `bottomKey(parts)`（`lib/live.ts` 的 `BottomParts`），把 `runningSeq` 和
`runningInputChars` 算进去；`live.test.ts` 加 4 个用例，第一条直接钉住这个 bug。

---

## 附三：成本账的文件名按对话分开（ADR 0071 的落地修正）

`perf.rs` 的文件名原来只有 `turn-NNNN-<phase>`。回合号是每个对话各自从头数的，所以两个对话
并行时互相覆盖，存下来的账也不知道属于哪一轮——上一轮读 perf 时已被误导过一次（`turn-0004`
与 `turn-0065` 混在同一目录）。现在文件名是 `turn-<session>-NNNN-<phase>.json`，`perf_report`
与 `report.ts` / `Chat.svelte` 传 `sessionId`；两条测试钉住“两个对话互不覆盖”和“对话出现在
文件名里”。
