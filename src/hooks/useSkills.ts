import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import {
  skillsApi,
  type AppSkill,
  type AppSkillsResponse,
  type CliProvidedSkill,
  type DiscoverableSkill,
  type GlobalSkill,
  type GlobalSkillsResponse,
  type ImportSkillSelection,
  type InstalledSkill,
  type SkillBackupEntry,
  type SkillUpdateInfo,
  type SkillsShLeaderboardResult,
  type SkillsShLeaderboardView,
  type SkillsShSearchResult,
} from "@/lib/api/skills";
import type { AppId, SkillAppId } from "@/lib/api/types";

export function useAppSkills(app: SkillAppId, enabled = true) {
  return useQuery({
    queryKey: ["skills", "app", app],
    queryFn: () => skillsApi.getForApp(app),
    staleTime: Infinity,
    placeholderData: keepPreviousData,
    enabled,
  });
}

export function useCliProvidedSkills(app: SkillAppId, enabled = true) {
  return useQuery({
    queryKey: ["skills", "provided", app],
    queryFn: () => skillsApi.getProvidedForApp(app),
    staleTime: Infinity,
    enabled,
  });
}

export function useAppSkillBackups(app: SkillAppId) {
  return useQuery({
    queryKey: ["skills", "backups", app],
    queryFn: () => skillsApi.getBackupsForApp(app),
    enabled: false,
  });
}

export function useGlobalSkills(enabled = true) {
  return useQuery({
    queryKey: ["skills", "global"],
    queryFn: () => skillsApi.getGlobal(),
    staleTime: Infinity,
    enabled,
  });
}

export function useGlobalSkillBackups() {
  return useQuery({
    queryKey: ["skills", "backups", "global"],
    queryFn: () => skillsApi.getGlobalBackups(),
    enabled: false,
  });
}

export function useInstallGlobalSkill() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (skill: DiscoverableSkill) => skillsApi.installGlobal(skill),
    onSuccess: (installedSkill) => {
      queryClient.setQueryData<GlobalSkillsResponse>(
        ["skills", "global"],
        (current) =>
          current
            ? { ...current, skills: [...current.skills, installedSkill] }
            : current,
      );
      queryClient.invalidateQueries({ queryKey: ["skills", "app"] });
    },
  });
}

export function useSetGlobalSkillLink() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      directory,
      app,
      enabled,
    }: {
      directory: string;
      app: SkillAppId;
      enabled: boolean;
    }) => skillsApi.setGlobalLink(directory, app, enabled),
    onSuccess: (updatedSkill, { app }) => {
      queryClient.setQueryData<GlobalSkillsResponse>(
        ["skills", "global"],
        (current) =>
          current
            ? {
                ...current,
                skills: current.skills.map((skill) =>
                  skill.directory === updatedSkill.directory
                    ? updatedSkill
                    : skill,
                ),
              }
            : current,
      );
      queryClient.invalidateQueries({ queryKey: ["skills", "app", app] });
    },
  });
}

export function useUninstallGlobalSkill() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (directory: string) => skillsApi.uninstallGlobal(directory),
    onSuccess: (_result, directory) => {
      queryClient.setQueryData<GlobalSkillsResponse>(
        ["skills", "global"],
        (current) =>
          current
            ? {
                ...current,
                skills: current.skills.filter(
                  (skill) => skill.directory !== directory,
                ),
              }
            : current,
      );
      queryClient.invalidateQueries({ queryKey: ["skills", "app"] });
      queryClient.invalidateQueries({
        queryKey: ["skills", "backups", "global"],
      });
    },
  });
}

export function useRestoreGlobalSkillBackup() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (backupId: string) => skillsApi.restoreGlobalBackup(backupId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills", "global"] });
      queryClient.invalidateQueries({ queryKey: ["skills", "app"] });
      queryClient.invalidateQueries({
        queryKey: ["skills", "backups", "global"],
      });
    },
  });
}

export function useInstallGlobalSkillsFromZip() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (filePath: string) => skillsApi.installFromZipGlobal(filePath),
    onSuccess: (installedSkills) => {
      queryClient.setQueryData<GlobalSkillsResponse>(
        ["skills", "global"],
        (current) =>
          current
            ? {
                ...current,
                skills: [...current.skills, ...installedSkills],
              }
            : current,
      );
      queryClient.invalidateQueries({ queryKey: ["skills", "app"] });
    },
  });
}

export function useDeleteGlobalSkillBackup() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (backupId: string) => skillsApi.deleteBackup(backupId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["skills", "backups", "global"],
      });
    },
  });
}

export function useCheckGlobalSkillUpdates() {
  return useQuery({
    queryKey: ["skills", "global", "updates"],
    queryFn: () => skillsApi.checkGlobalUpdates(),
    enabled: false,
  });
}

export function useUpdateGlobalSkill() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => skillsApi.updateGlobalSkill(id),
    onSuccess: (updatedSkill) => {
      queryClient.setQueryData<GlobalSkillsResponse>(
        ["skills", "global"],
        (current) =>
          current
            ? {
                ...current,
                skills: current.skills.map((skill) =>
                  skill.directory === updatedSkill.directory
                    ? updatedSkill
                    : skill,
                ),
              }
            : current,
      );
      queryClient.invalidateQueries({
        queryKey: ["skills", "global", "updates"],
      });
      queryClient.invalidateQueries({ queryKey: ["skills", "app"] });
      queryClient.invalidateQueries({
        queryKey: ["skills", "backups", "global"],
      });
    },
  });
}

export function useDeleteSkillBackup(app: SkillAppId) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (backupId: string) => skillsApi.deleteBackup(backupId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["skills", "backups", app],
      });
    },
  });
}

export function useCheckAppSkillUpdates(app: SkillAppId) {
  return useQuery({
    queryKey: ["skills", "app", app, "updates"],
    queryFn: () => skillsApi.checkAppUpdates(app),
    enabled: false,
  });
}

export function useUpdateAppSkill() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ app, id }: { app: SkillAppId; id: string }) =>
      skillsApi.updateAppSkill(app, id),
    onSuccess: (updatedSkill, { app }) => {
      queryClient.setQueryData<AppSkillsResponse>(
        ["skills", "app", app],
        (current) =>
          current
            ? {
                ...current,
                skills: current.skills.map((skill) =>
                  skill.directory === updatedSkill.directory
                    ? updatedSkill
                    : skill,
                ),
              }
            : current,
      );
      queryClient.invalidateQueries({
        queryKey: ["skills", "app", app, "updates"],
      });
      queryClient.invalidateQueries({
        queryKey: ["skills", "backups", app],
      });
    },
  });
}

export function useDiscoverableSkills() {
  return useQuery({
    queryKey: ["skills", "discoverable"],
    queryFn: () => skillsApi.discoverAvailable(),
    staleTime: Infinity,
    placeholderData: keepPreviousData,
  });
}

export function useInstallSkill() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      skill,
      currentApp,
    }: {
      skill: DiscoverableSkill;
      currentApp: SkillAppId;
    }) => skillsApi.installForApp(currentApp, skill),
    onSuccess: (installedSkill, { currentApp }) => {
      queryClient.setQueryData<AppSkillsResponse>(
        ["skills", "app", currentApp],
        (current) =>
          current
            ? { ...current, skills: [...current.skills, installedSkill] }
            : current,
      );
      queryClient.invalidateQueries({ queryKey: ["skills", "global"] });
    },
  });
}

export function useUninstallSkill() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ app, directory }: { app: SkillAppId; directory: string }) =>
      skillsApi.uninstallForApp(app, directory),
    onSuccess: (_result, { app, directory }) => {
      queryClient.setQueryData<AppSkillsResponse>(
        ["skills", "app", app],
        (current) =>
          current
            ? {
                ...current,
                skills: current.skills.filter(
                  (skill) => skill.directory !== directory,
                ),
              }
            : current,
      );
      queryClient.invalidateQueries({
        queryKey: ["skills", "backups", app],
      });
      queryClient.invalidateQueries({ queryKey: ["skills", "global"] });
    },
  });
}

export function useRestoreSkillBackup() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      backupId,
      currentApp,
    }: {
      backupId: string;
      currentApp: SkillAppId;
    }) => skillsApi.restoreBackupForApp(currentApp, backupId),
    onSuccess: (_skill, { currentApp }) => {
      queryClient.invalidateQueries({
        queryKey: ["skills", "app", currentApp],
      });
      queryClient.invalidateQueries({
        queryKey: ["skills", "backups", currentApp],
      });
    },
  });
}

export function useSkillRepos() {
  return useQuery({
    queryKey: ["skills", "repos"],
    queryFn: () => skillsApi.getRepos(),
  });
}

export function useAddSkillRepo() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: skillsApi.addRepo,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills", "repos"] });
      queryClient.invalidateQueries({ queryKey: ["skills", "discoverable"] });
    },
  });
}

export function useRemoveSkillRepo() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ owner, name }: { owner: string; name: string }) =>
      skillsApi.removeRepo(owner, name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills", "repos"] });
      queryClient.invalidateQueries({ queryKey: ["skills", "discoverable"] });
    },
  });
}

export function useInstallSkillsFromZip() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      filePath,
      currentApp,
    }: {
      filePath: string;
      currentApp: SkillAppId;
    }) => skillsApi.installFromZipForApp(currentApp, filePath),
    onSuccess: (installedSkills, { currentApp }) => {
      queryClient.setQueryData<AppSkillsResponse>(
        ["skills", "app", currentApp],
        (current) =>
          current
            ? {
                ...current,
                skills: [...current.skills, ...installedSkills],
              }
            : current,
      );
      queryClient.invalidateQueries({ queryKey: ["skills", "global"] });
    },
  });
}

export function useSearchSkillsSh(query: string, limit: number) {
  return useQuery({
    queryKey: ["skills", "skillssh", query, limit],
    queryFn: () => skillsApi.searchSkillsSh(query, limit),
    enabled: query.length >= 2,
    staleTime: 5 * 60 * 1000,
  });
}

export function useSkillsShLeaderboard(
  view: SkillsShLeaderboardView,
  limit: number,
) {
  return useQuery({
    queryKey: ["skills", "skillssh", "leaderboard", view, limit],
    queryFn: () => skillsApi.getSkillsShLeaderboard(view, limit),
    staleTime: 5 * 60 * 1000,
  });
}

export function useSkillsShPublisher(owner: string) {
  return useQuery({
    queryKey: ["skills", "skillssh", "publisher", owner],
    queryFn: () => skillsApi.getSkillsShPublisher(owner),
    enabled: Boolean(owner),
    staleTime: 10 * 60 * 1000,
  });
}

export function useSkillsShRepository(owner: string, repository: string) {
  return useQuery({
    queryKey: ["skills", "skillssh", "repository", owner, repository],
    queryFn: () => skillsApi.getSkillsShRepository(owner, repository),
    enabled: Boolean(owner && repository),
    staleTime: 10 * 60 * 1000,
  });
}

export function useSkillsShDetail(
  repoOwner: string,
  repoName: string,
  skillId: string,
) {
  return useQuery({
    queryKey: ["skills", "skillssh", "detail", repoOwner, repoName, skillId],
    queryFn: () => skillsApi.getSkillsShDetail(repoOwner, repoName, skillId),
    enabled: Boolean(repoOwner && repoName && skillId),
    staleTime: 10 * 60 * 1000,
  });
}

export type {
  AppId,
  AppSkill,
  AppSkillsResponse,
  CliProvidedSkill,
  DiscoverableSkill,
  GlobalSkill,
  GlobalSkillsResponse,
  ImportSkillSelection,
  InstalledSkill,
  SkillAppId,
  SkillBackupEntry,
  SkillUpdateInfo,
  SkillsShLeaderboardResult,
  SkillsShLeaderboardView,
  SkillsShSearchResult,
};
