import { createInstance } from "i18next";
import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import zh from "@/i18n/locales/zh.json";
import zhTW from "@/i18n/locales/zh-TW.json";
import ja from "@/i18n/locales/ja.json";

describe("OpenCode built-in agent translations", () => {
  it.each([
    ["en", en, "Built-in", "Restore defaults"],
    ["zh", zh, "内置", "恢复默认"],
    ["zh-TW", zhTW, "內建", "還原預設"],
    ["ja", ja, "組み込み", "既定に戻す"],
  ] as const)(
    "resolves the actual component keys in %s",
    async (language, resource, badge, reset) => {
      const i18n = createInstance();
      await i18n.init({
        lng: language,
        fallbackLng: false,
        resources: { [language]: { translation: resource } },
      });
      expect(i18n.t("agents.source.builtIn")).toBe(badge);
      expect(i18n.t("agents.reset.button")).toBe(reset);
      for (const key of [
        "agents.source.builtInHint",
        "agents.reset.title",
        "agents.reset.message",
        "agents.notifications.reset",
        "agents.form.builtInPromptPlaceholder",
        ...[
          "build",
          "plan",
          "general",
          "explore",
          "compaction",
          "title",
          "summary",
        ].map((id) => `agents.builtInDescriptions.${id}`),
      ]) {
        expect(i18n.exists(key), key).toBe(true);
        expect(i18n.t(key), key).not.toBe(key);
        expect(i18n.t(key), key).not.toBe("");
      }
      // OpenClaw has a separate nested agents section; it must remain intact.
      expect(i18n.exists("openclaw.agents.primaryModel")).toBe(true);
    },
  );
});
