import { describe, expect, it } from "vitest";
import {
  buildOmoSlimProfilePreview,
  extractOmoSlimActivePresetAgents,
  OMO_SLIM_BUILTIN_AGENTS,
  OMO_SLIM_DISABLEABLE_AGENTS,
} from "./omo";

describe("OMO Slim preset configuration", () => {
  const settings = {
    otherFields: {
      $schema: "https://slim.schema",
      preset: "openai",
      presets: {
        openai: {
          orchestrator: {
            model: "openai/gpt-5.6-sol",
            variant: "medium",
          },
        },
        "opencode-go": {
          orchestrator: { model: "opencode-go/glm-5.2" },
        },
      },
    },
  };

  it("loads agents from the active preset in existing provider records", () => {
    expect(extractOmoSlimActivePresetAgents(settings)).toEqual(
      settings.otherFields.presets.openai,
    );
  });

  it("matches the nine built-in agents from oh-my-opencode-slim 2.1.1", () => {
    expect(OMO_SLIM_BUILTIN_AGENTS.map((agent) => agent.key)).toEqual([
      "orchestrator",
      "oracle",
      "librarian",
      "explorer",
      "designer",
      "fixer",
      "observer",
      "council",
      "councillor",
    ]);
    expect(
      OMO_SLIM_DISABLEABLE_AGENTS.map((agent) => agent.value),
    ).not.toContain("orchestrator");
    expect(
      OMO_SLIM_DISABLEABLE_AGENTS.map((agent) => agent.value),
    ).not.toContain("councillor");
  });

  it("writes edited agents back to the active preset in the preview", () => {
    const editedAgents = {
      orchestrator: { model: "openai/gpt-5.6-terra", variant: "high" },
    };
    const preview = buildOmoSlimProfilePreview(
      editedAgents,
      JSON.stringify(settings.otherFields),
    );

    expect(preview).not.toHaveProperty("agents");
    expect(preview.presets).toEqual({
      openai: editedAgents,
      "opencode-go": settings.otherFields.presets["opencode-go"],
    });
  });

  it("replaces the active preset with an empty object when all agents are cleared", () => {
    const preview = buildOmoSlimProfilePreview(
      {},
      JSON.stringify(settings.otherFields),
    );

    expect(preview).not.toHaveProperty("agents");
    expect(preview.presets).toEqual({
      openai: {},
      "opencode-go": settings.otherFields.presets["opencode-go"],
    });
  });
});
