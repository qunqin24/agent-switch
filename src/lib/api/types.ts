// 前端统一使用 AppId 作为应用标识（与后端命令参数 `app` 一致）
export type AppId =
  | "claude"
  | "claude-desktop"
  | "codex"
  | "gemini"
  | "opencode"
  | "openclaw"
  | "hermes"
  | "pi";

export type McpAppId = Exclude<AppId, "claude-desktop" | "openclaw" | "pi">;
export type SkillAppId = Exclude<AppId, "claude-desktop" | "openclaw">;
