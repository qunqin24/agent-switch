import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AgentsPanel } from "@/components/agents/AgentsPanel";
import type { OpenCodeAgentDocument } from "@/lib/api/opencodeAgents";

const agentMocks = vi.hoisted(() => ({
  list: vi.fn(),
  save: vi.fn(),
  delete: vi.fn(),
  listMcpServerIds: vi.fn(),
  open: vi.fn(),
  listModels: vi.fn(),
}));

vi.mock("@/lib/api/opencodeAgents", () => ({
  opencodeAgentsApi: {
    list: (...args: unknown[]) => agentMocks.list(...args),
    save: (...args: unknown[]) => agentMocks.save(...args),
    delete: (...args: unknown[]) => agentMocks.delete(...args),
    listMcpServerIds: () => agentMocks.listMcpServerIds(),
  },
}));

vi.mock("@/lib/api", () => ({
  providersApi: {
    listOpenCodeModelsForOmo: () => agentMocks.listModels(),
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => agentMocks.open(...args),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

const explorer: OpenCodeAgentDocument = {
  id: "explorer",
  scope: "global",
  filePath: "/tmp/agents/explorer.md",
  frontmatter: {
    description: "Search code quickly",
    mode: "subagent",
    model: "opencode-go/deepseek-v4-flash",
    variant: "high",
    temperature: 0.1,
    permission: {
      edit: "deny",
      external_directory: { "*": "ask" },
    },
    custom_option: { enabled: true },
  },
  prompt: "You are Explorer.",
};

const builtInPermissionKeys = [
  "read",
  "edit",
  "glob",
  "grep",
  "list",
  "bash",
  "task",
  "external_directory",
  "todowrite",
  "webfetch",
  "websearch",
  "lsp",
  "skill",
  "question",
  "doom_loop",
];

beforeEach(() => {
  agentMocks.list.mockReset().mockResolvedValue([explorer]);
  agentMocks.save
    .mockReset()
    .mockImplementation(
      async (_location: unknown, agent: OpenCodeAgentDocument) => ({
        ...agent,
        filePath: `/tmp/agents/${agent.id}.md`,
      }),
    );
  agentMocks.delete.mockReset().mockResolvedValue(undefined);
  agentMocks.listMcpServerIds
    .mockReset()
    .mockResolvedValue(["context7", "github"]);
  agentMocks.open.mockReset().mockResolvedValue(null);
  agentMocks.listModels.mockReset().mockResolvedValue([
    {
      value: "opencode-go/deepseek-v4-flash",
      providerId: "opencode-go",
      modelId: "deepseek-v4-flash",
      name: "DeepSeek V4 Flash",
      variants: ["high"],
    },
  ]);
});

describe("AgentsPanel", () => {
  it("loads native agents and preserves advanced fields when saving", async () => {
    render(<AgentsPanel onOpenChange={vi.fn()} />);

    const agentButton = await screen.findByRole("button", {
      name: /explorer/i,
    });
    fireEvent.click(agentButton);

    expect(screen.getByDisplayValue("explorer")).toBeInTheDocument();
    const description = screen.getByDisplayValue("Search code quickly");
    fireEvent.change(description, { target: { value: "Search every file" } });
    fireEvent.click(screen.getByRole("button", { name: "common.save" }));

    await waitFor(() => expect(agentMocks.save).toHaveBeenCalledTimes(1));
    const [, saved, originalId] = agentMocks.save.mock.calls[0] as [
      unknown,
      OpenCodeAgentDocument,
      string,
    ];
    expect(originalId).toBe("explorer");
    expect(saved.frontmatter).toMatchObject({
      description: "Search every file",
      mode: "subagent",
      model: "opencode-go/deepseek-v4-flash",
      variant: "high",
      permission: {
        edit: "deny",
        external_directory: { "*": "ask" },
      },
      custom_option: { enabled: true },
    });
  });

  it("loads project agents after choosing a project folder", async () => {
    agentMocks.open.mockResolvedValue("/tmp/project");
    render(<AgentsPanel onOpenChange={vi.fn()} />);

    await screen.findByRole("button", { name: /explorer/i });
    fireEvent.click(
      screen.getByRole("button", { name: "agents.scope.project" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "agents.scope.chooseProject" }),
    );

    await waitFor(() =>
      expect(agentMocks.list).toHaveBeenCalledWith({
        scope: "project",
        projectDir: "/tmp/project",
      }),
    );
  });

  it("shows OMO Slim agents as read-only", async () => {
    agentMocks.list.mockResolvedValue([{ ...explorer, managedBy: "omo-slim" }]);
    render(<AgentsPanel onOpenChange={vi.fn()} />);

    fireEvent.click(
      await screen.findByRole("button", {
        name: /explorer/i,
      }),
    );

    expect(screen.getAllByText("agents.source.omoSlim")).toHaveLength(2);
    expect(screen.getByText("agents.source.readOnlyHint")).toBeInTheDocument();
    expect(screen.getByDisplayValue("explorer")).toBeDisabled();
    expect(screen.getByRole("button", { name: "common.save" })).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "common.delete" }),
    ).not.toBeInTheDocument();
  });

  it("preserves OpenCode's all-mode default for legacy agents without mode", async () => {
    const { mode: _mode, ...legacyFrontmatter } = explorer.frontmatter;
    agentMocks.list.mockResolvedValue([
      { ...explorer, frontmatter: legacyFrontmatter },
    ]);
    render(<AgentsPanel onOpenChange={vi.fn()} />);

    fireEvent.click(
      await screen.findByRole("button", {
        name: /explorer/i,
      }),
    );
    expect(
      screen.getByRole("button", { name: "agents.mode.all" }),
    ).toHaveAttribute("aria-pressed", "true");

    fireEvent.change(screen.getByDisplayValue("Search code quickly"), {
      target: { value: "Search legacy code" },
    });
    fireEvent.click(screen.getByRole("button", { name: "common.save" }));

    await waitFor(() => expect(agentMocks.save).toHaveBeenCalledTimes(1));
    const [, saved] = agentMocks.save.mock.calls[0] as [
      unknown,
      OpenCodeAgentDocument,
    ];
    expect(saved.frontmatter.mode).toBe("all");
  });

  it("saves server-wide MCP permissions without dropping other permission rules", async () => {
    render(<AgentsPanel onOpenChange={vi.fn()} />);

    fireEvent.click(
      await screen.findByRole("button", {
        name: /explorer/i,
      }),
    );
    await screen.findByText("github");

    const allowButtons = screen.getAllByRole("button", {
      name: "agents.permissions.allow",
    });
    fireEvent.click(allowButtons[allowButtons.length - 1]);
    fireEvent.click(screen.getByRole("button", { name: "common.save" }));

    await waitFor(() => expect(agentMocks.save).toHaveBeenCalledTimes(1));
    const [, saved] = agentMocks.save.mock.calls[0] as [
      unknown,
      OpenCodeAgentDocument,
    ];
    expect(saved.frontmatter.permission).toMatchObject({
      external_directory: { "*": "ask" },
      "github_*": "allow",
    });
  });

  it("applies one action to every built-in tool permission", async () => {
    render(<AgentsPanel onOpenChange={vi.fn()} />);

    fireEvent.click(
      await screen.findByRole("button", {
        name: /explorer/i,
      }),
    );
    const denyAllButton = screen.getByRole("button", {
      name: "agents.permissions.bulkDeny",
    });
    fireEvent.click(denyAllButton);
    expect(denyAllButton).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "common.save" }));

    await waitFor(() => expect(agentMocks.save).toHaveBeenCalledTimes(1));
    const [, saved] = agentMocks.save.mock.calls[0] as [
      unknown,
      OpenCodeAgentDocument,
    ];
    const permission = saved.frontmatter.permission as Record<string, unknown>;
    for (const key of builtInPermissionKeys) {
      expect(permission[key]).toBe("deny");
    }
  });

  it("applies one action to every MCP server while preserving specific tool rules", async () => {
    agentMocks.list.mockResolvedValue([
      {
        ...explorer,
        frontmatter: {
          ...explorer.frontmatter,
          permission: {
            edit: "deny",
            external_directory: { "*": "ask" },
            github_search: "deny",
          },
        },
      },
    ]);
    render(<AgentsPanel onOpenChange={vi.fn()} />);

    fireEvent.click(
      await screen.findByRole("button", {
        name: /explorer/i,
      }),
    );
    await screen.findByText("github");
    fireEvent.click(
      screen.getByRole("button", {
        name: "agents.mcpPermissions.bulkAllow",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "common.save" }));

    await waitFor(() => expect(agentMocks.save).toHaveBeenCalledTimes(1));
    const [, saved] = agentMocks.save.mock.calls[0] as [
      unknown,
      OpenCodeAgentDocument,
    ];
    expect(saved.frontmatter.permission).toMatchObject({
      external_directory: { "*": "ask" },
      "context7_*": "allow",
      "github_*": "allow",
      github_search: "deny",
    });
  });
});
