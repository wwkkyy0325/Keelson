import { derived, writable } from "svelte/store";

export type Locale = "zh" | "en";

const zh = {
  "wizard.step": "第 {current} / {total} 步",
  "wizard.back": "上一步",
  "wizard.key.title": "连接模型",
  "wizard.key.subtitle": "密钥只保存在系统钥匙串里，不会写进配置文件。",
  "wizard.key.placeholder": "粘贴 API Key",
  "wizard.key.show": "显示",
  "wizard.key.hide": "隐藏",
  "wizard.key.next": "下一步",
  "wizard.key.offline": "先用离线模式",
  "wizard.key.offlineHint": "离线模式不调用模型，适合先看看界面。",
  "wizard.model.title": "接入方式",
  "wizard.model.subtitle": "默认值已经填好。只填了密钥的话，可以直接继续。",
  "wizard.model.provider": "提供方",
  "wizard.model.baseUrl": "接口地址",
  "wizard.model.model": "模型",
  "wizard.model.anthropic": "Anthropic 兼容",
  "wizard.model.openai": "OpenAI 兼容",
  "wizard.model.mock": "离线 mock",
  "wizard.model.next": "下一步",
  "wizard.language.title": "语言",
  "wizard.language.subtitle": "界面语言，进入应用后也能随时切换。",
  "wizard.language.zh": "中文",
  "wizard.language.en": "English",
  "wizard.language.next": "下一步",
  "wizard.finish.title": "准备好了",
  "wizard.finish.subtitle": "工作区在应用里切换，不需要现在选。",
  "wizard.finish.key": "密钥",
  "wizard.finish.keySaved": "已保存到系统钥匙串",
  "wizard.finish.offline": "离线模式",
  "wizard.finish.model": "模型",
  "wizard.finish.language": "语言",
  "wizard.finish.start": "开始使用",
  "wizard.finish.busy": "正在进入…",
  "wizard.problems": "{count} 个 rig 有问题",
  "settings.title": "设置",
  "settings.model": "模型与密钥",
  "settings.modelHint": "修改后会重新连接当前工作区。",
  "settings.key": "密钥",
  "settings.keySaved": "已保存，留空则不修改",
  "settings.keySavedHint": "密钥在系统钥匙串里；留空表示不修改。",
  "settings.language": "语言",
  "settings.data": "信息",
  "settings.workspace": "当前工作区",
  "settings.sessionHint": "会话按工作区分别保存，切换目录不会混在一起。",
  "settings.save": "保存并重连",
  "settings.saving": "正在保存…",
  "settings.close": "关闭",
  "settings.error.key": "请先填写密钥",
  "wizard.error.key": "请先填写密钥",
  "wizard.error.model": "接口地址和模型名不能为空",
  "wizard.error.save": "保存失败：{message}",
  "app.empty": "还没有对话，直接输入就好。",
  "app.placeholder": "发给 Keelson…",
  "app.send": "发送",
  "app.cancel": "取消",
  "app.loadOlder": "加载更早的回合",
  "app.loading": "加载中…",
  "app.working": "思考中…",
  "app.chooseWorkspace": "切换工作区",
  "app.settings": "设置",
  "app.switchLanguage": "Switch to English",
  "app.dismiss": "关闭",
  "app.permission": "需要确认",
  "app.turnFailed": "回合失败",
  "app.unknownError": "出了点问题",
  "app.loadingApp": "正在打开 Keelson…",
} as const;

type Key = keyof typeof zh;

const en: Record<Key, string> = {
  "wizard.step": "Step {current} / {total}",
  "wizard.back": "Back",
  "wizard.key.title": "Connect a model",
  "wizard.key.subtitle":
    "The key is stored in the OS keychain, never in the config file.",
  "wizard.key.placeholder": "Paste an API key",
  "wizard.key.show": "Show",
  "wizard.key.hide": "Hide",
  "wizard.key.next": "Continue",
  "wizard.key.offline": "Use offline mode",
  "wizard.key.offlineHint": "Offline mode calls no model; good for a first look.",
  "wizard.model.title": "Endpoint",
  "wizard.model.subtitle":
    "The defaults are filled in. If you only have a key, just continue.",
  "wizard.model.provider": "Provider",
  "wizard.model.baseUrl": "Base URL",
  "wizard.model.model": "Model",
  "wizard.model.anthropic": "Anthropic-compatible",
  "wizard.model.openai": "OpenAI-compatible",
  "wizard.model.mock": "Offline mock",
  "wizard.model.next": "Continue",
  "wizard.language.title": "Language",
  "wizard.language.subtitle":
    "The interface language. You can switch it later too.",
  "wizard.language.zh": "中文",
  "wizard.language.en": "English",
  "wizard.language.next": "Continue",
  "wizard.finish.title": "Ready",
  "wizard.finish.subtitle":
    "The workspace is chosen inside the app, not here.",
  "wizard.finish.key": "Key",
  "wizard.finish.keySaved": "Saved in the OS keychain",
  "wizard.finish.offline": "Offline mode",
  "wizard.finish.model": "Model",
  "wizard.finish.language": "Language",
  "wizard.finish.start": "Start using Keelson",
  "wizard.finish.busy": "Opening…",
  "wizard.problems": "{count} rig problem(s)",
  "settings.title": "Settings",
  "settings.model": "Model & key",
  "settings.modelHint": "Saving reconnects the current workspace.",
  "settings.key": "Key",
  "settings.keySaved": "Saved; leave empty to keep",
  "settings.keySavedHint": "The key lives in the OS keychain; leave it empty to keep.",
  "settings.language": "Language",
  "settings.data": "Info",
  "settings.workspace": "Current workspace",
  "settings.sessionHint":
    "Sessions are stored per workspace, so switching directories never mixes them.",
  "settings.save": "Save & reconnect",
  "settings.saving": "Saving…",
  "settings.close": "Close",
  "settings.error.key": "Enter a key first",
  "wizard.error.key": "Enter a key first",
  "wizard.error.model": "The base URL and model must not be empty",
  "wizard.error.save": "Could not save: {message}",
  "app.empty": "No conversation yet. Start typing.",
  "app.placeholder": "Message Keelson…",
  "app.send": "Send",
  "app.cancel": "Cancel",
  "app.loadOlder": "Load older turns",
  "app.loading": "Loading…",
  "app.working": "Working…",
  "app.chooseWorkspace": "Switch workspace",
  "app.settings": "Settings",
  "app.switchLanguage": "切换到中文",
  "app.dismiss": "Dismiss",
  "app.permission": "Needs confirmation",
  "app.turnFailed": "Turn failed",
  "app.unknownError": "Something went wrong",
  "app.loadingApp": "Opening Keelson…",
};

const messages: Record<Locale, Record<Key, string>> = { zh, en };

function format(template: string, vars?: Record<string, string | number>): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (_, name: string) =>
    name in vars ? String(vars[name]) : `{${name}}`,
  );
}

export function translate(
  language: Locale,
  key: Key,
  vars?: Record<string, string | number>,
): string {
  return format(messages[language][key], vars);
}

export const locale = writable<Locale>("zh");

/// `$t("key", { vars })` in a component; reactive on the locale store.
export const t = derived(locale, ($locale) => {
  return (key: Key, vars?: Record<string, string | number>) =>
    translate($locale, key, vars);
});

const zhPermission: Record<string, string> = {
  deny: "拒绝",
  allow_once: "仅本次允许",
  allow_scope: "本路径允许",
  full_auto: "全自动",
};

export const permissionLabel = derived(locale, ($locale) => {
  return (id: string, fallback: string) =>
    $locale === "zh" && zhPermission[id] ? zhPermission[id] : fallback;
});
