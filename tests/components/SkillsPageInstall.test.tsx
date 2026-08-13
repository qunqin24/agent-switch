import { createRef } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach } from "vitest";

import {
  SkillsPage,
  type SkillsPageHandle,
} from "@/components/skills/SkillsPage";
import type {
  SkillsShDiscoverableSkill,
  SkillsShLeaderboardResult,
  SkillsShLeaderboardView,
  SkillsShPublisherDetail,
  SkillsShRepositoryDetail,
  SkillsShSearchResult,
  SkillsShSkillDetail,
} from "@/lib/api/skills";

const { installMutateAsyncMock, installGlobalMutateAsyncMock } = vi.hoisted(
  () => ({
    installMutateAsyncMock: vi.fn(),
    installGlobalMutateAsyncMock: vi.fn(),
  }),
);

// Stable cache so repeated renders see referentially-equal query data.
const searchCache = new Map<
  string,
  {
    data: SkillsShSearchResult | undefined;
    isLoading: boolean;
    isFetching: boolean;
    isError: boolean;
    refetch: ReturnType<typeof vi.fn>;
  }
>();

const leaderboardCache = new Map<
  SkillsShLeaderboardView,
  {
    data: SkillsShLeaderboardResult;
    isLoading: boolean;
    isFetching: boolean;
    isError: boolean;
    refetch: ReturnType<typeof vi.fn>;
  }
>();

const emptyLeaderboardQueries: Record<
  SkillsShLeaderboardView,
  {
    data: SkillsShLeaderboardResult;
    isLoading: boolean;
    isFetching: boolean;
    isError: boolean;
    refetch: ReturnType<typeof vi.fn>;
  }
> = {
  "all-time": {
    data: {
      skills: [],
      resultCount: 0,
      totalSkills: 0,
      allTimeTotal: 1_044_078,
      view: "all-time",
    },
    isLoading: false,
    isFetching: false,
    isError: false,
    refetch: vi.fn(),
  },
  trending: {
    data: {
      skills: [],
      resultCount: 0,
      totalSkills: 0,
      allTimeTotal: 1_044_078,
      view: "trending",
    },
    isLoading: false,
    isFetching: false,
    isError: false,
    refetch: vi.fn(),
  },
  hot: {
    data: {
      skills: [],
      resultCount: 0,
      totalSkills: 0,
      allTimeTotal: 1_044_078,
      view: "hot",
    },
    isLoading: false,
    isFetching: false,
    isError: false,
    refetch: vi.fn(),
  },
};

const setSearchResult = (
  query: string,
  result: SkillsShSearchResult | undefined,
) => {
  searchCache.set(query, {
    data: result,
    isLoading: false,
    isFetching: false,
    isError: false,
    refetch: vi.fn(),
  });
};

const setLeaderboardResult = (
  view: SkillsShLeaderboardView,
  result: SkillsShLeaderboardResult,
) => {
  leaderboardCache.set(view, {
    data: result,
    isLoading: false,
    isFetching: false,
    isError: false,
    refetch: vi.fn(),
  });
};

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

vi.mock("@/hooks/useSkills", () => ({
  useAppSkills: () => ({
    data: {
      app: "claude",
      skillsDir: "/tmp/claude/skills",
      skills: [],
    },
    refetch: vi.fn(),
  }),
  useGlobalSkills: () => ({
    data: {
      skillsDir: "/tmp/.agents/skills",
      directApps: {
        claude: false,
        codex: true,
        gemini: false,
        opencode: false,
        openclaw: false,
        hermes: false,
      },
      skills: [],
    },
    refetch: vi.fn(),
  }),
  useInstallSkill: () => ({
    mutateAsync: installMutateAsyncMock,
  }),
  useInstallGlobalSkill: () => ({
    mutateAsync: installGlobalMutateAsyncMock,
  }),
  useSearchSkillsSh: (query: string) => {
    const cached = searchCache.get(query);
    if (cached) return cached;
    return {
      data: undefined,
      isLoading: false,
      isFetching: false,
      isError: false,
      refetch: vi.fn(),
    };
  },
  useSkillsShLeaderboard: (view: SkillsShLeaderboardView) =>
    leaderboardCache.get(view) ?? emptyLeaderboardQueries[view],
  useSkillsShPublisher: (owner: string) => ({
    data: {
      owner,
      sourceCount: 1,
      skillCount: 1,
      totalInstalls: "2.8M",
      sources: [
        {
          name: "repo-a",
          skillSummary: "1 skill: agent-browser",
          installs: "2.8M",
        },
      ],
    } satisfies SkillsShPublisherDetail,
    isLoading: false,
    isFetching: false,
    isError: false,
    refetch: vi.fn(),
  }),
  useSkillsShRepository: (owner: string, repository: string) => ({
    data: {
      owner,
      repository,
      skillCount: 1,
      totalInstalls: "2.8M",
      skills: [
        {
          skillId: "agent-browser",
          name: "Agent Browser",
          installs: 2_800_000,
          installsLabel: "2.8M",
        },
      ],
    } satisfies SkillsShRepositoryDetail,
    isLoading: false,
    isFetching: false,
    isError: false,
    refetch: vi.fn(),
  }),
  useSkillsShDetail: () => ({
    data: {
      topic: "Agent workflows",
      summaryHtml:
        "<p><strong>Discover and install specialized agent skills.</strong></p>",
      contentHtml:
        "<h1>Find Skills</h1><p>This skill helps you discover skills.</p>",
      githubStars: "27.6K",
      firstSeen: "Jan 26, 2026",
      securityAudits: [{ provider: "Socket", status: "Pass" }],
    } satisfies SkillsShSkillDetail,
    isLoading: false,
    isFetching: false,
    isError: false,
    refetch: vi.fn(),
  }),
}));

const makeSkillsShSkill = (
  overrides: Partial<SkillsShDiscoverableSkill> = {},
): SkillsShDiscoverableSkill => ({
  key: "agent-browser:owner-a:repo-a",
  name: "Agent Browser",
  directory: "agent-browser",
  repoOwner: "owner-a",
  repoName: "repo-a",
  repoBranch: "main",
  installs: 100,
  weeklyInstalls: [10, 12, 9, 15, 18, 20, 21, 24],
  isOfficial: false,
  readmeUrl: "https://skills.sh/owner-a/repo-a/agent-browser",
  detailUrl: "https://skills.sh/owner-a/repo-a/agent-browser",
  ...overrides,
});

describe("SkillsPage - skills.sh install (regression)", () => {
  beforeEach(() => {
    installMutateAsyncMock.mockReset();
    installMutateAsyncMock.mockResolvedValue({});
    installGlobalMutateAsyncMock.mockReset();
    installGlobalMutateAsyncMock.mockResolvedValue({});
    searchCache.clear();
    leaderboardCache.clear();
  });

  it("shows all-time by default and switches to trending and hot tabs", async () => {
    const allTimeSkill = makeSkillsShSkill({
      key: "all-time:owner-a:repo-a",
      name: "All Time Skill",
    });
    const trendingSkill = makeSkillsShSkill({
      key: "trending:owner-b:repo-b",
      name: "Trending Skill",
      repoOwner: "owner-b",
      repoName: "repo-b",
    });
    const hotSkill = makeSkillsShSkill({
      key: "hot:owner-c:repo-c",
      name: "Hot Skill",
      repoOwner: "owner-c",
      repoName: "repo-c",
    });
    setLeaderboardResult("all-time", {
      skills: [allTimeSkill],
      resultCount: 1,
      totalSkills: 9_575,
      allTimeTotal: 1_044_078,
      view: "all-time",
    });
    setLeaderboardResult("trending", {
      skills: [trendingSkill],
      resultCount: 1,
      totalSkills: 9_392,
      allTimeTotal: 1_044_078,
      view: "trending",
    });
    setLeaderboardResult("hot", {
      skills: [hotSkill],
      resultCount: 1,
      totalSkills: 4_708,
      allTimeTotal: 1_044_078,
      view: "hot",
    });

    render(<SkillsPage initialApp="claude" />);
    const user = userEvent.setup();

    expect(await screen.findByText("All Time Skill")).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: "skills.skillssh.tabs.allTime" }),
    ).toHaveAttribute("aria-selected", "true");

    await user.click(
      screen.getByRole("tab", { name: "skills.skillssh.tabs.trending" }),
    );
    expect(await screen.findByText("Trending Skill")).toBeInTheDocument();

    await user.click(
      screen.getByRole("tab", { name: "skills.skillssh.tabs.hot" }),
    );
    expect(await screen.findByText("Hot Skill")).toBeInTheDocument();
  });

  it("installs the second skill when two results share the same directory", async () => {
    const first = makeSkillsShSkill({
      key: "agent-browser:owner-a:repo-a",
      name: "Agent Browser A",
      repoOwner: "owner-a",
      repoName: "repo-a",
    });
    const second = makeSkillsShSkill({
      key: "agent-browser:owner-b:repo-b",
      name: "Agent Browser B",
      repoOwner: "owner-b",
      repoName: "repo-b",
    });

    setSearchResult("agent", {
      skills: [first, second],
      resultCount: 2,
      query: "agent",
    });

    const ref = createRef<SkillsPageHandle>();
    render(<SkillsPage ref={ref} initialApp="claude" />);

    const user = userEvent.setup();

    // Type a query and submit
    const input = screen.getByPlaceholderText(
      "skills.skillssh.searchPlaceholder",
    );
    await user.type(input, "agent");
    await user.click(screen.getByRole("button", { name: "skills.search" }));

    // Wait for both rows to render
    await waitFor(() => {
      expect(screen.getByText("Agent Browser A")).toBeInTheDocument();
      expect(screen.getByText("Agent Browser B")).toBeInTheDocument();
    });

    // Click install on the SECOND row (Agent Browser B)
    const secondRow = screen.getByTestId(`skill-row-${second.key}`);
    const installButton = secondRow.querySelector(
      'button[aria-label="skills.install"]',
    ) as HTMLButtonElement;
    expect(installButton).not.toBeNull();
    await user.click(installButton);

    // Verify the SECOND skill was passed to the install mutation, not the first
    await waitFor(() => {
      expect(installMutateAsyncMock).toHaveBeenCalledTimes(1);
    });
    const callArgs = installMutateAsyncMock.mock.calls[0][0];
    expect(callArgs.skill.repoOwner).toBe("owner-b");
    expect(callArgs.skill.repoName).toBe("repo-b");
    expect(callArgs.skill.name).toBe("Agent Browser B");
  });

  it("installs into the global library without writing to the active CLI", async () => {
    const globalSkill = makeSkillsShSkill({
      key: "global-browser:owner-global:repo-global",
      name: "Global Browser",
      repoOwner: "owner-global",
      repoName: "repo-global",
    });
    setSearchResult("global", {
      skills: [globalSkill],
      resultCount: 1,
      query: "global",
    });

    render(<SkillsPage initialApp="codex" installTarget="global" />);
    const user = userEvent.setup();
    const input = screen.getByPlaceholderText(
      "skills.skillssh.searchPlaceholder",
    );
    await user.type(input, "global");
    await user.click(screen.getByRole("button", { name: "skills.search" }));
    await user.click(
      await screen.findByRole("button", { name: "skills.install" }),
    );

    await waitFor(() =>
      expect(installGlobalMutateAsyncMock).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "Global Browser",
          repoOwner: "owner-global",
        }),
      ),
    );
    expect(installMutateAsyncMock).not.toHaveBeenCalled();
  });

  it("opens the Skill detail inside Agent Switch instead of an external link", async () => {
    const skill = makeSkillsShSkill({
      name: "Skill Details",
      detailUrl: "https://skills.sh/vercel-labs/skills/find-skills",
    });
    setSearchResult("details", {
      skills: [skill],
      resultCount: 1,
      query: "details",
    });

    render(<SkillsPage initialApp="claude" />);
    const user = userEvent.setup();
    await user.type(
      screen.getByPlaceholderText("skills.skillssh.searchPlaceholder"),
      "details",
    );
    await user.click(screen.getByRole("button", { name: "skills.search" }));
    await user.click(
      await screen.findByRole("button", {
        name: "skills.skillssh.openDetails",
      }),
    );

    expect(
      await screen.findByRole("heading", { name: "Skill Details" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Discover and install specialized agent skills."),
    ).toBeInTheDocument();
    expect(screen.getByText("27.6K")).toBeInTheDocument();
  });

  it("navigates through clickable Skill breadcrumbs without leaving the app", async () => {
    const skill = makeSkillsShSkill({ name: "Skill Details" });
    setSearchResult("details", {
      skills: [skill],
      resultCount: 1,
      query: "details",
    });

    render(<SkillsPage initialApp="claude" />);
    const user = userEvent.setup();
    await user.type(
      screen.getByPlaceholderText("skills.skillssh.searchPlaceholder"),
      "details",
    );
    await user.click(screen.getByRole("button", { name: "skills.search" }));
    await user.click(
      await screen.findByRole("button", {
        name: "skills.skillssh.openDetails",
      }),
    );

    await user.click(screen.getByRole("button", { name: /^repo-a$/ }));
    expect(
      await screen.findByRole("heading", { name: "owner-a/repo-a" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^owner-a$/ }));
    expect(
      await screen.findByRole("heading", { name: "owner-a" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^repo-a 1 skill:/ }));
    expect(
      await screen.findByRole("heading", { name: "owner-a/repo-a" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^skills$/ }));
    expect(
      screen.getByRole("tablist", { name: "skills.skillssh.leaderboard" }),
    ).toBeInTheDocument();
  });
});
