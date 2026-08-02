import { createRef } from "react";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import SkillsPanel, {
  type SkillsPanelHandle,
} from "@/components/skills/SkillsPanel";

const refetchSkillsMock = vi.fn();
const refetchGlobalSkillsMock = vi.fn();
const refetchProvidedSkillsMock = vi.fn();
const onScopeChangeMock = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

vi.mock("@/hooks/useSkills", () => ({
  useAppSkills: (appId: string) => {
    const data =
      appId === "opencode"
        ? {
            app: "opencode",
            skillsDir: "/tmp/opencode/skills",
            skills: [
              {
                id: "opencode:local:clonedeps",
                name: "clonedeps",
                description: "OpenCode native skill",
                directory: "clonedeps",
                path: "/tmp/opencode/skills/clonedeps",
                isSymlink: false,
                managedGlobally: false,
                globalSource: false,
                installedAt: 1,
                updatedAt: 0,
              },
              {
                id: "opencode:global:app-architect",
                name: "app-architect",
                description: "Linked from the shared global directory",
                directory: "app-architect",
                path: "/tmp/opencode/skills/app-architect",
                isSymlink: true,
                linkTarget: "/tmp/global/skills/app-architect",
                managedGlobally: true,
                globalSource: false,
                installedAt: 1,
                updatedAt: 0,
              },
            ],
          }
        : appId === "codex"
          ? {
              app: "codex",
              skillsDir: "/tmp/codex/skills",
              skills: [
                {
                  id: "codex:local:test-skill",
                  name: "Codex Only Skill",
                  description: "Read from the Codex native directory",
                  directory: "test-skill",
                  path: "/tmp/codex/skills/test-skill",
                  isSymlink: false,
                  managedGlobally: false,
                  globalSource: false,
                  installedAt: 1,
                  updatedAt: 0,
                },
              ],
            }
          : {
              app: appId,
              skillsDir: `/tmp/${appId}/skills`,
              skills: [],
            };
    return {
      data,
      isLoading: false,
      isFetching: false,
      refetch: refetchSkillsMock,
    };
  },
  useGlobalSkills: () => ({
    data: {
      skillsDir: "/tmp/global/skills",
      directApps: {
        claude: false,
        codex: true,
        gemini: true,
        opencode: true,
        openclaw: false,
        hermes: false,
      },
      skills: [
        {
          id: "global:app-architect",
          name: "app-architect",
          description: "Shared global skill",
          directory: "app-architect",
          path: "/tmp/global/skills/app-architect",
          apps: {
            claude: false,
            codex: true,
            gemini: false,
            opencode: true,
            openclaw: false,
            hermes: false,
          },
          installedAt: 1,
          updatedAt: 0,
        },
        {
          id: "global:article-writing",
          name: "article-writing",
          description: "Available through the shared global directory",
          directory: "article-writing",
          path: "/tmp/global/skills/article-writing",
          apps: {
            claude: false,
            codex: true,
            gemini: false,
            opencode: true,
            openclaw: false,
            hermes: false,
          },
          installedAt: 1,
          updatedAt: 0,
        },
      ],
    },
    isLoading: false,
    isFetching: false,
    refetch: refetchGlobalSkillsMock,
  }),
  useCliProvidedSkills: (appId: string) => ({
    data:
      appId === "opencode"
        ? [
            {
              id: "opencode:builtin:customize-opencode",
              name: "customize-opencode",
              description: "Configure OpenCode itself",
              directory: "customize-opencode",
              path: "<built-in>",
              source: { kind: "builtin" },
            },
            {
              id: "opencode:plugin:oh-my-opencode-slim:clonedeps",
              name: "clonedeps",
              description: "OpenCode native skill",
              directory: "clonedeps",
              path: "/tmp/opencode/skills/clonedeps/SKILL.md",
              source: {
                kind: "plugin",
                pluginName: "oh-my-opencode-slim",
              },
            },
          ]
        : appId === "codex"
          ? [
              {
                id: "codex:builtin:openai-docs",
                name: "openai-docs",
                description: "Use official OpenAI documentation",
                directory: "openai-docs",
                path: "/tmp/codex/skills/.system/openai-docs",
                source: { kind: "builtin" },
              },
            ]
          : [],
    isLoading: false,
    isFetching: false,
    refetch: refetchProvidedSkillsMock,
  }),
  useAppSkillBackups: () => ({
    data: [],
    isFetching: false,
    refetch: vi.fn(),
  }),
  useDeleteSkillBackup: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
  useUninstallSkill: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: undefined,
  }),
  useRestoreSkillBackup: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
  useInstallSkillsFromZip: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
  useCheckAppSkillUpdates: (appId: string) => ({
    data:
      appId === "opencode"
        ? [
            {
              id: "opencode:local:clonedeps",
              name: "clonedeps",
              remoteHash: "updated",
            },
          ]
        : [],
    isFetching: false,
    refetch: vi.fn(),
  }),
  useUpdateAppSkill: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: undefined,
  }),
}));

describe("SkillsPanel", () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn();
    refetchSkillsMock.mockReset();
    refetchSkillsMock.mockResolvedValue({});
    refetchGlobalSkillsMock.mockReset();
    refetchGlobalSkillsMock.mockResolvedValue({});
    refetchProvidedSkillsMock.mockReset();
    refetchProvidedSkillsMock.mockResolvedValue({});
    onScopeChangeMock.mockReset();
  });

  it("renders all effective Codex skill roots and refreshes them", async () => {
    const ref = createRef<SkillsPanelHandle>();
    render(
      <SkillsPanel ref={ref} appId="codex" onScopeChange={onScopeChangeMock} />,
    );

    expect(
      screen.getByText("/tmp/codex/skills · /tmp/global/skills"),
    ).toBeInTheDocument();
    expect(screen.getByText("Codex Only Skill")).toBeInTheDocument();
    expect(
      screen.getByText("Read from the Codex native directory"),
    ).toBeInTheDocument();
    expect(screen.getByText("openai-docs")).toBeInTheDocument();

    await act(async () => {
      ref.current?.refresh();
    });

    expect(refetchSkillsMock).toHaveBeenCalledTimes(1);
    expect(refetchGlobalSkillsMock).toHaveBeenCalledTimes(1);
    expect(refetchProvidedSkillsMock).toHaveBeenCalledTimes(1);
  });

  it("includes the shared global directory for Gemini CLI", () => {
    render(<SkillsPanel appId="gemini" onScopeChange={onScopeChangeMock} />);

    expect(
      screen.getByText("/tmp/gemini/skills · /tmp/global/skills"),
    ).toBeInTheDocument();
    expect(screen.getByText("article-writing")).toBeInTheDocument();
  });

  it("merges OpenCode native and shared global skills without duplicates", async () => {
    const ref = createRef<SkillsPanelHandle>();
    render(
      <SkillsPanel
        ref={ref}
        appId="opencode"
        onScopeChange={onScopeChangeMock}
      />,
    );

    expect(
      screen.getByText("/tmp/opencode/skills · /tmp/global/skills"),
    ).toBeInTheDocument();
    expect(screen.getByText("clonedeps")).toBeInTheDocument();
    expect(screen.getByText("article-writing")).toBeInTheDocument();
    expect(screen.getByText("customize-opencode")).toBeInTheDocument();
    expect(screen.getByText("skills.builtin")).toBeInTheDocument();
    expect(screen.getByText("skills.pluginProvided")).toBeInTheDocument();
    expect(screen.getAllByText("app-architect")).toHaveLength(1);
    expect(
      screen.getByText("Linked from the shared global directory"),
    ).toBeInTheDocument();
    expect(screen.queryByText("Shared global skill")).not.toBeInTheDocument();
    expect(
      screen
        .getByText("customize-opencode")
        .closest(".group")
        ?.querySelector('button[title="skills.uninstall"]'),
    ).toBeNull();
    expect(
      screen
        .getByText("clonedeps")
        .closest(".group")
        ?.querySelector('button[title="skills.uninstall"]'),
    ).toBeNull();
    expect(
      screen
        .getByText("clonedeps")
        .closest(".group")
        ?.querySelector('button[title="skills.update"]'),
    ).toBeNull();

    await act(async () => {
      ref.current?.refresh();
    });

    expect(refetchSkillsMock).toHaveBeenCalledTimes(1);
    expect(refetchGlobalSkillsMock).toHaveBeenCalledTimes(1);
    expect(refetchProvidedSkillsMock).toHaveBeenCalledTimes(1);
  });

  it("switches the active CLI from the installed Skills heading", async () => {
    const user = userEvent.setup();
    render(<SkillsPanel appId="codex" onScopeChange={onScopeChangeMock} />);

    const appSwitcher = screen.getByRole("combobox", {
      name: "skills.switchScope",
    });
    appSwitcher.focus();
    await user.keyboard("[Enter]");
    await user.keyboard("[ArrowDown][ArrowDown][Enter]");

    expect(onScopeChangeMock).toHaveBeenCalledWith({
      kind: "app",
      app: "opencode",
    });
  });

  it("switches from a CLI to the global Skills library", async () => {
    const user = userEvent.setup();
    render(<SkillsPanel appId="codex" onScopeChange={onScopeChangeMock} />);

    const scopeSwitcher = screen.getByRole("combobox", {
      name: "skills.switchScope",
    });
    scopeSwitcher.focus();
    await user.keyboard("[Enter][Home][Enter]");

    expect(onScopeChangeMock).toHaveBeenCalledWith({ kind: "global" });
  });
});
