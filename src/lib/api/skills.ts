import { invoke } from "@tauri-apps/api/core";

import type { AppId, SkillAppId } from "@/lib/api/types";

export type AppType =
  | "claude"
  | "claude-desktop"
  | "codex"
  | "gemini"
  | "opencode"
  | "openclaw"
  | "hermes"
  | "pi";

/** Skill 应用启用状态 */
export interface SkillApps {
  claude: boolean;
  "claude-desktop"?: boolean;
  codex: boolean;
  gemini: boolean;
  opencode: boolean;
  openclaw: boolean;
  hermes: boolean;
  pi: boolean;
}

/** 已安装的 Skill（v3.10.0+ 统一结构） */
export interface InstalledSkill {
  id: string;
  name: string;
  description?: string;
  directory: string;
  repoOwner?: string;
  repoName?: string;
  repoBranch?: string;
  readmeUrl?: string;
  apps: SkillApps;
  installedAt: number;
  contentHash?: string;
  updatedAt: number;
}

export interface SkillUninstallResult {
  backupPath?: string;
}

export interface AppSkill {
  id: string;
  name: string;
  description?: string;
  directory: string;
  path: string;
  isSymlink: boolean;
  linkTarget?: string;
  managedGlobally: boolean;
  globalSource: boolean;
  providedBy?: CliProvidedSkillSource;
  repoOwner?: string;
  repoName?: string;
  repoBranch?: string;
  readmeUrl?: string;
  installedAt: number;
  contentHash?: string;
  updatedAt: number;
}

export type CliProvidedSkillSource =
  | { kind: "builtin" }
  | { kind: "plugin"; pluginName: string };

export interface CliProvidedSkill {
  id: string;
  name: string;
  description?: string;
  directory: string;
  path: string;
  source: CliProvidedSkillSource;
}

export interface AppSkillsResponse {
  app: SkillAppId;
  skillsDir: string;
  skills: AppSkill[];
}

export interface GlobalSkill {
  id: string;
  name: string;
  description?: string;
  directory: string;
  path: string;
  repoOwner?: string;
  repoName?: string;
  repoBranch?: string;
  readmeUrl?: string;
  apps: SkillApps;
  installedAt: number;
  contentHash?: string;
  updatedAt: number;
}

export interface GlobalSkillsResponse {
  skillsDir: string;
  directApps: SkillApps;
  skills: GlobalSkill[];
}

export interface SkillBackupEntry {
  backupId: string;
  backupPath: string;
  createdAt: number;
  skill: InstalledSkill;
  sourceApp?: SkillAppId;
}

/** 可发现的 Skill（来自仓库） */
export interface DiscoverableSkill {
  key: string;
  name: string;
  description: string;
  directory: string;
  readmeUrl?: string;
  repoOwner: string;
  repoName: string;
  repoBranch: string;
}

/** 未管理的 Skill（用于导入） */
export interface UnmanagedSkill {
  directory: string;
  name: string;
  description?: string;
  foundIn: string[];
  path: string;
}

/** 导入已有 Skill 时提交的应用启用状态 */
export interface ImportSkillSelection {
  directory: string;
  apps: SkillApps;
}

/** 技能对象（兼容旧 API） */
export interface Skill {
  key: string;
  name: string;
  description: string;
  directory: string;
  readmeUrl?: string;
  installed: boolean;
  repoOwner?: string;
  repoName?: string;
  repoBranch?: string;
}

/** Skill 更新信息 */
export interface SkillUpdateInfo {
  id: string;
  name: string;
  currentHash?: string;
  remoteHash: string;
}

/** 存储位置迁移结果 */
export interface MigrationResult {
  migratedCount: number;
  skippedCount: number;
  errors: string[];
}

/** skills.sh 可发现的技能 */
export interface SkillsShDiscoverableSkill {
  key: string;
  name: string;
  directory: string;
  repoOwner: string;
  repoName: string;
  repoBranch: string;
  installs: number;
  weeklyInstalls: number[];
  isOfficial: boolean;
  readmeUrl?: string;
  detailUrl: string;
}

export interface SkillsShSecurityAudit {
  provider: string;
  status: string;
}

/** skills.sh 公开详情页 */
export interface SkillsShSkillDetail {
  topic?: string;
  summaryHtml: string;
  contentHtml: string;
  githubStars?: string;
  firstSeen?: string;
  securityAudits: SkillsShSecurityAudit[];
}

export interface SkillsShSourceSummary {
  name: string;
  skillSummary: string;
  installs: string;
}

/** skills.sh 发布者页 */
export interface SkillsShPublisherDetail {
  owner: string;
  sourceCount: number;
  skillCount: number;
  totalInstalls: string;
  sources: SkillsShSourceSummary[];
}

export interface SkillsShRepositorySkill {
  skillId: string;
  name: string;
  installs: number;
  installsLabel: string;
}

/** skills.sh 仓库页 */
export interface SkillsShRepositoryDetail {
  owner: string;
  repository: string;
  skillCount: number;
  totalInstalls: string;
  skills: SkillsShRepositorySkill[];
}

/** skills.sh 搜索结果 */
export interface SkillsShSearchResult {
  skills: SkillsShDiscoverableSkill[];
  resultCount: number;
  query: string;
}

export type SkillsShLeaderboardView = "all-time" | "trending" | "hot";

/** skills.sh 公开榜单结果 */
export interface SkillsShLeaderboardResult {
  skills: SkillsShDiscoverableSkill[];
  resultCount: number;
  totalSkills: number;
  allTimeTotal: number;
  view: SkillsShLeaderboardView;
}

/** 仓库配置 */
export interface SkillRepo {
  owner: string;
  name: string;
  branch: string;
  enabled: boolean;
}

// ========== API ==========

export const skillsApi = {
  // ========== 按 CLI 原生目录管理 ==========

  /** 读取指定 CLI 原生 Skills 目录。 */
  async getForApp(app: SkillAppId): Promise<AppSkillsResponse> {
    return await invoke("get_app_skills", { app });
  },

  /** 读取 CLI 或其插件提供的只读 Skills。 */
  async getProvidedForApp(app: SkillAppId): Promise<CliProvidedSkill[]> {
    return await invoke("get_cli_provided_skills", { app });
  },

  /** 读取当前 CLI 可恢复的备份。 */
  async getBackupsForApp(app: SkillAppId): Promise<SkillBackupEntry[]> {
    return await invoke("get_app_skill_backups", { app });
  },

  /** 将仓库中的 Skill 直接安装到指定 CLI。 */
  async installForApp(
    app: SkillAppId,
    skill: DiscoverableSkill,
  ): Promise<AppSkill> {
    return await invoke("install_app_skill", { app, skill });
  },

  /** 仅从指定 CLI 卸载 Skill。 */
  async uninstallForApp(
    app: SkillAppId,
    directory: string,
  ): Promise<SkillUninstallResult> {
    return await invoke("uninstall_app_skill", { app, directory });
  },

  /** 将备份恢复到指定 CLI。 */
  async restoreBackupForApp(
    app: SkillAppId,
    backupId: string,
  ): Promise<AppSkill> {
    return await invoke("restore_app_skill_backup", { app, backupId });
  },

  /** 将 ZIP 中的 Skills 直接安装到指定 CLI。 */
  async installFromZipForApp(
    app: SkillAppId,
    filePath: string,
  ): Promise<AppSkill[]> {
    return await invoke("install_app_skills_from_zip", { app, filePath });
  },

  /** 检查指定 CLI 原生目录中可追踪来源的 Skills 更新。 */
  async checkAppUpdates(app: SkillAppId): Promise<SkillUpdateInfo[]> {
    return await invoke("check_app_skill_updates", { app });
  },

  /** 更新指定 CLI 原生目录中的单个 Skill。 */
  async updateAppSkill(app: SkillAppId, id: string): Promise<AppSkill> {
    return await invoke("update_app_skill", { app, id });
  },

  // ========== 全局 Skills 库 ==========

  /** 读取 ~/.agents/skills 全局目录及其实际软链接状态。 */
  async getGlobal(): Promise<GlobalSkillsResponse> {
    return await invoke("get_global_skills");
  },

  /** 读取全局 Skill 备份。 */
  async getGlobalBackups(): Promise<SkillBackupEntry[]> {
    return await invoke("get_global_skill_backups");
  },

  /** 安装到 ~/.agents/skills，不自动创建额外的 CLI 软链接。 */
  async installGlobal(skill: DiscoverableSkill): Promise<GlobalSkill> {
    return await invoke("install_global_skill", { skill });
  },

  /** 创建或移除全局 Skill 到指定 CLI 的软链接。 */
  async setGlobalLink(
    directory: string,
    app: SkillAppId,
    enabled: boolean,
  ): Promise<GlobalSkill> {
    return await invoke("set_global_skill_link", {
      directory,
      app,
      enabled,
    });
  },

  /** 从全局库卸载，并移除其创建的所有 CLI 软链接。 */
  async uninstallGlobal(directory: string): Promise<SkillUninstallResult> {
    return await invoke("uninstall_global_skill", { directory });
  },

  /** 将备份恢复到全局库。 */
  async restoreGlobalBackup(backupId: string): Promise<GlobalSkill> {
    return await invoke("restore_global_skill_backup", { backupId });
  },

  /** 将 ZIP 中的 Skills 安装到全局库。 */
  async installFromZipGlobal(filePath: string): Promise<GlobalSkill[]> {
    return await invoke("install_global_skills_from_zip", { filePath });
  },

  /** 检查全局库中可追踪来源的 Skills 更新。 */
  async checkGlobalUpdates(): Promise<SkillUpdateInfo[]> {
    return await invoke("check_global_skill_updates");
  },

  /** 更新全局库中的单个 Skill。 */
  async updateGlobalSkill(id: string): Promise<GlobalSkill> {
    return await invoke("update_global_skill", { id });
  },

  // ========== 统一管理 API (v3.10.0+) ==========

  /** 获取所有已安装的 Skills */
  async getInstalled(): Promise<InstalledSkill[]> {
    return await invoke("get_installed_skills");
  },

  /** 获取可恢复的 Skill 备份列表 */
  async getBackups(): Promise<SkillBackupEntry[]> {
    return await invoke("get_skill_backups");
  },

  /** 删除 Skill 备份 */
  async deleteBackup(backupId: string): Promise<boolean> {
    return await invoke("delete_skill_backup", { backupId });
  },

  /** 安装 Skill（统一安装） */
  async installUnified(
    skill: DiscoverableSkill,
    currentApp: AppId,
  ): Promise<InstalledSkill> {
    return await invoke("install_skill_unified", { skill, currentApp });
  },

  /** 卸载 Skill（统一卸载） */
  async uninstallUnified(id: string): Promise<SkillUninstallResult> {
    return await invoke("uninstall_skill_unified", { id });
  },

  /** 从备份恢复 Skill */
  async restoreBackup(
    backupId: string,
    currentApp: AppId,
  ): Promise<InstalledSkill> {
    return await invoke("restore_skill_backup", { backupId, currentApp });
  },

  /** 切换 Skill 的应用启用状态 */
  async toggleApp(id: string, app: AppId, enabled: boolean): Promise<boolean> {
    return await invoke("toggle_skill_app", { id, app, enabled });
  },

  /** 扫描未管理的 Skills */
  async scanUnmanaged(): Promise<UnmanagedSkill[]> {
    return await invoke("scan_unmanaged_skills");
  },

  /** 从应用目录导入 Skills */
  async importFromApps(
    imports: ImportSkillSelection[],
  ): Promise<InstalledSkill[]> {
    return await invoke("import_skills_from_apps", { imports });
  },

  /** 发现可安装的 Skills（从仓库获取） */
  async discoverAvailable(): Promise<DiscoverableSkill[]> {
    return await invoke("discover_available_skills");
  },

  /** 检查 Skills 更新 */
  async checkUpdates(): Promise<SkillUpdateInfo[]> {
    return await invoke("check_skill_updates");
  },

  /** 更新单个 Skill */
  async updateSkill(id: string): Promise<InstalledSkill> {
    return await invoke("update_skill", { id });
  },

  /** 迁移 Skill 存储位置 */
  async migrateStorage(
    target: "cc_switch" | "unified",
  ): Promise<MigrationResult> {
    return await invoke("migrate_skill_storage", { target });
  },

  /** 搜索 skills.sh 公共目录 */
  async searchSkillsSh(
    query: string,
    limit: number,
  ): Promise<SkillsShSearchResult> {
    return await invoke("search_skills_sh", { query, limit });
  },

  /** 读取 skills.sh 总榜、24 小时趋势或最热榜。 */
  async getSkillsShLeaderboard(
    view: SkillsShLeaderboardView,
    limit: number,
  ): Promise<SkillsShLeaderboardResult> {
    return await invoke("get_skills_sh_leaderboard", { view, limit });
  },

  /** 读取 skills.sh 公开发布者页。 */
  async getSkillsShPublisher(owner: string): Promise<SkillsShPublisherDetail> {
    return await invoke("get_skills_sh_publisher", { owner });
  },

  /** 读取 skills.sh 公开仓库页。 */
  async getSkillsShRepository(
    owner: string,
    repository: string,
  ): Promise<SkillsShRepositoryDetail> {
    return await invoke("get_skills_sh_repository", { owner, repository });
  },

  /** 读取 skills.sh 公开 Skill 详情。 */
  async getSkillsShDetail(
    repoOwner: string,
    repoName: string,
    skillId: string,
  ): Promise<SkillsShSkillDetail> {
    return await invoke("get_skills_sh_detail", {
      repoOwner,
      repoName,
      skillId,
    });
  },

  // ========== 兼容旧 API ==========

  /** 获取技能列表（兼容旧 API） */
  async getAll(app: AppId = "claude"): Promise<Skill[]> {
    if (app === "claude") {
      return await invoke("get_skills");
    }
    return await invoke("get_skills_for_app", { app });
  },

  /** 安装技能（兼容旧 API） */
  async install(directory: string, app: AppId = "claude"): Promise<boolean> {
    if (app === "claude") {
      return await invoke("install_skill", { directory });
    }
    return await invoke("install_skill_for_app", { app, directory });
  },

  /** 卸载技能（兼容旧 API） */
  async uninstall(
    directory: string,
    app: AppId = "claude",
  ): Promise<SkillUninstallResult> {
    if (app === "claude") {
      return await invoke("uninstall_skill", { directory });
    }
    return await invoke("uninstall_skill_for_app", { app, directory });
  },

  // ========== 仓库管理 ==========

  /** 获取仓库列表 */
  async getRepos(): Promise<SkillRepo[]> {
    return await invoke("get_skill_repos");
  },

  /** 添加仓库 */
  async addRepo(repo: SkillRepo): Promise<boolean> {
    return await invoke("add_skill_repo", { repo });
  },

  /** 删除仓库 */
  async removeRepo(owner: string, name: string): Promise<boolean> {
    return await invoke("remove_skill_repo", { owner, name });
  },

  // ========== ZIP 安装 ==========

  /** 打开 ZIP 文件选择对话框 */
  async openZipFileDialog(): Promise<string | null> {
    return await invoke("open_zip_file_dialog");
  },

  /** 从 ZIP 文件安装 Skills */
  async installFromZip(
    filePath: string,
    currentApp: AppId,
  ): Promise<InstalledSkill[]> {
    return await invoke("install_skills_from_zip", { filePath, currentApp });
  },
};
