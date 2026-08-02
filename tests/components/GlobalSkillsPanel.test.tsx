import { createRef } from "react";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import GlobalSkillsPanel, {
  type GlobalSkillsPanelHandle,
} from "@/components/skills/GlobalSkillsPanel";

const refetchSkillsMock = vi.fn();
const setLinkMock = vi.fn();
const onScopeChangeMock = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

vi.mock("@/hooks/useSkills", () => ({
  useGlobalSkills: () => ({
    data: {
      skillsDir: "/tmp/.agents/skills",
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
          id: "global:local:test-skill",
          name: "Shared Global Skill",
          description: "Linked only when selected",
          directory: "test-skill",
          path: "/tmp/.agents/skills/test-skill",
          apps: {
            claude: false,
            codex: true,
            gemini: true,
            opencode: true,
            hermes: false,
          },
          installedAt: 1,
          updatedAt: 0,
        },
      ],
    },
    isLoading: false,
    isFetching: false,
    refetch: refetchSkillsMock,
  }),
  useGlobalSkillBackups: () => ({
    data: [],
    isFetching: false,
    refetch: vi.fn(),
  }),
  useSetGlobalSkillLink: () => ({
    mutateAsync: setLinkMock,
    isPending: false,
  }),
  useUninstallGlobalSkill: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
  useRestoreGlobalSkillBackup: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
  useDeleteGlobalSkillBackup: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
  useInstallGlobalSkillsFromZip: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
  useCheckGlobalSkillUpdates: () => ({
    data: [],
    isFetching: false,
    refetch: vi.fn(),
  }),
  useUpdateGlobalSkill: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: undefined,
  }),
}));

describe("GlobalSkillsPanel", () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn();
    refetchSkillsMock.mockReset();
    refetchSkillsMock.mockResolvedValue({});
    setLinkMock.mockReset();
    setLinkMock.mockResolvedValue({});
    onScopeChangeMock.mockReset();
  });

  it("uses the skills.sh global directory and links only non-native CLIs", async () => {
    const ref = createRef<GlobalSkillsPanelHandle>();
    render(<GlobalSkillsPanel ref={ref} onScopeChange={onScopeChangeMock} />);

    expect(screen.getByText("/tmp/.agents/skills")).toBeInTheDocument();
    expect(screen.getByText("Shared Global Skill")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Codex" }));
    fireEvent.click(screen.getByRole("button", { name: "Gemini" }));
    fireEvent.click(screen.getByRole("button", { name: "OpenCode" }));
    expect(setLinkMock).not.toHaveBeenCalled();

    expect(screen.getByRole("button", { name: "Codex" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Gemini" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "OpenCode" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    fireEvent.click(screen.getByRole("button", { name: "Claude" }));
    await waitFor(() =>
      expect(setLinkMock).toHaveBeenCalledWith({
        directory: "test-skill",
        app: "claude",
        enabled: true,
      }),
    );

    await act(async () => {
      ref.current?.refresh();
    });
    expect(refetchSkillsMock).toHaveBeenCalledTimes(1);
  });

  it("switches from the global library back to a CLI", async () => {
    const user = userEvent.setup();
    render(<GlobalSkillsPanel onScopeChange={onScopeChangeMock} />);

    const scopeSwitcher = screen.getByRole("combobox", {
      name: "skills.switchScope",
    });
    scopeSwitcher.focus();
    await user.keyboard("[Enter][ArrowDown][ArrowDown][Enter]");

    expect(onScopeChangeMock).toHaveBeenCalledWith({
      kind: "app",
      app: "codex",
    });
  });
});
