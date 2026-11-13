# Presets: one-click switching of model + rig + permission

Status: implemented 2026-11-13.

## Goal

Switching between recurring setups (local qwen / DeepSeek cloud / explainer
with a vision model) takes three places today: Settings for the connection,
the status bar for permission, the rig chip for the rig — and the rig chip
only exists in an empty conversation. One named preset should restore all of
them, from one control, in one click.

## Non-goals

- No rig editor and no per-rig behaviour toggles. A preset stores which rig
  file runs, never what it does.
- No secrets in presets. A preset names a **key slot** (`keySlot`, defaulting
  to `providerId`), so one key can serve several models (DeepSeek flash and
  another DeepSeek model) and two self-hosted services can keep two slots
  (`volcengine`, `custom`) without overwriting each other.
- No kernel change.
- No mid-conversation rig change. A conversation with a turn keeps its rig
  (`rig_for_session` pins it in the session document); a preset's rig applies
  to the next conversation. This is today's rule and needs no ADR change.

## Data model

`DesktopConfig` gains two fields (container already has `#[serde(default)]`):

```rust
pub struct Preset {
    pub id: String,            // stable, generated on first save
    pub name: String,
    pub rig: String,
    pub provider: String,      // openai-compatible | anthropic-compatible
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub model: String,
    pub model_context: Option<u32>,
    pub key_slot: Option<String>,     // empty means provider_id
    pub permission: Option<String>,   // workspace_auto | full_auto | None
    pub search_url: Option<String>,
}

pub key_slot: Option<String>,          // slot of the live connection
pub presets: Vec<Preset>,
pub active_preset: Option<String>,
```

`active_key_slot()` resolves the live slot (`key_slot` or `provider_id`) and
the open path loads the model key by that slot, so switching presets between
two models on one key re-reads the same keychain item.

`active_preset` is only a marker; the flat config fields stay authoritative,
so a deleted or edited preset can never leave the app in a broken state. An
install from before this feature starts with `presets: []` and the current
connection becomes the first preset on first save.

## Commands (desktop/src-tauri/src/commands.rs)

- `save_preset(name)` — copy the current config into a preset; update the one
  with the same name, else append. Sets `active_preset`.
- `apply_preset(id)` — validate the rig still exists, copy the preset's fields
  into the config, set `active_preset`, save.
- `delete_preset(id)` — remove; clear `active_preset` if it pointed there.

`SetupState` already carries the connection fields; add `presets` and
`activePreset` next to them (one struct change, one protocol.ts type).

## UI

Status bar, left side, before the permission chip:

```
[ 预设 · 本地 ▾ ]  [ 全自动 ▾ ]  · 编码（在线）· OpenAI 兼容 qwen3.8-flash-next-iq3_s
```

The menu reuses the permission popover pattern:

- one row per preset: name, and `rig · provider model · permission` below it;
  the active one is marked
- a footer input + `保存当前为预设` (name defaults to the model name)
- `管理…` opens Settings

Settings gains a `预设` section: the list with a delete button per row and
`从当前设置新建`. This is the only place presets are removed or renamed.

The existing rig chip stays for empty conversations; when presets exist it is
redundant but harmless, and it is the fallback when no preset was ever saved.

## Switching flow

1. `apply_preset(id)` writes the connection, rig, permission and search URL to
   the config.
2. If only `permission` differs, call the existing `savePermission` path and
   reload the status; no reopen.
3. Otherwise `reopenSession(workspace, sessionId)` — the same call Settings
   uses after Save. History is preserved; the next turn uses the new model.
4. The rig part applies when the conversation has no turns (no session
   document yet, so `rig_for_session` falls back to the config). A conversation
   with turns keeps its rig; the menu marks such a preset `新会话生效` when its
   rig differs from the running one.
5. Errors (missing rig, missing key for the provider) surface in the existing
   banner; the config is written only after validation.

## Change list

1. [x] `config.rs`: `Preset` struct (`keySlot` included), `key_slot`,
   `presets`, `active_preset`, serde defaults; tests for an old config and a
   preset round-trip.
2. [x] `commands.rs`: `save_preset`, `apply_preset`, `delete_preset`,
   `copy_preset` with two unit tests, `active_key_slot` used by open and
   setup.
3. [x] `protocol.ts` + `api.ts`: `Preset` type, `keySlot` on `save_model`,
   three wrappers.
4. [x] `StatusBar.svelte`: preset chip and menu with inline save and
   "manage" entry.
5. [x] `SettingsPanel.svelte`: presets section (list, delete, create), the
   manual key-slot field.
6. [x] `Chat.svelte` + `App.svelte`: apply → reopen the same conversation;
   refresh the setup after settings close, preset save and apply.
7. [x] i18n keys (zh/en) and the status-bar/settings styles.
8. [x] `Wizard.svelte`: key-slot field so a first manual endpoint can name
   its own slot.

Verification: desktop 49 tests, `clippy -D warnings`, frontend 23 tests,
`svelte-check` 0 errors, `vite build`. A smoke run against a temporary
`KEELSON_HOME` with three seeded presets (two DeepSeek models on one key, one
local endpoint on `custom`) rendered the chip as `DS flash` in the status bar.

Not done on purpose: switching a preset mid-conversation does not change the
rig; the conversation keeps the rig it started with and the preset's rig
lands on the next conversation.

## Alternatives considered

- **Preset as a config file per setup, app restarts to switch.** Cheaper to
  write, but every switch restarts the app and loses the open conversation.
  Rejected.
- **Put the preset list in Settings only.** One more click for the most
  frequent action; the status bar already owns the other two axes (permission,
  context), so the chip belongs there.
- **Include the workspace in a preset.** Tempting for "video project +
  explainer", but a workspace is a project, not a connection; the recent-
  project switcher already covers it. Left out.
