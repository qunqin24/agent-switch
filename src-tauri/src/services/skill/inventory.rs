use super::*;

impl SkillService {
    // ========== 路径管理 ==========

    pub(super) fn global_skills_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context(format_skill_error(
            "GET_HOME_DIR_FAILED",
            &[],
            Some("checkPermission"),
        ))?;
        Ok(home.join(".agents").join("skills"))
    }

    /// 获取 skills.sh 规范全局 Skills 目录（~/.agents/skills/）。
    pub fn get_global_skills_dir() -> Result<PathBuf> {
        let dir = Self::global_skills_path()?;
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 获取 SSOT 目录（根据设置返回 ~/.agentswitch/skills/ 或 ~/.agents/skills/）
    pub fn get_ssot_dir() -> Result<PathBuf> {
        let location = crate::settings::get_skill_storage_location();
        let dir = match location {
            SkillStorageLocation::CcSwitch => get_app_config_dir().join("skills"),
            SkillStorageLocation::Unified => {
                let home = dirs::home_dir().context(format_skill_error(
                    "GET_HOME_DIR_FAILED",
                    &[],
                    Some("checkPermission"),
                ))?;
                home.join(".agents").join("skills")
            }
        };
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 获取 Skill 卸载备份目录（~/.agentswitch/skill-backups/）
    pub(super) fn get_backup_dir() -> Result<PathBuf> {
        let dir = get_app_config_dir().join("skill-backups");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 获取应用的 skills 目录
    pub fn get_app_skills_dir(app: &AppType) -> Result<PathBuf> {
        // 目录覆盖：优先使用用户在 settings.json 中配置的 override 目录
        match app {
            AppType::Claude => {
                if let Some(custom) = crate::settings::get_claude_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::ClaudeDesktop => {}
            AppType::Codex => {
                if let Some(custom) = crate::settings::get_codex_override_dir() {
                    if custom.file_name().and_then(|name| name.to_str()) == Some(".codex") {
                        if let Some(home) = custom.parent() {
                            return Ok(home.join(".agents").join("skills"));
                        }
                    }
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Gemini => {
                if let Some(custom) = crate::settings::get_gemini_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::OpenCode => {
                if let Some(custom) = crate::settings::get_opencode_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::OpenClaw => {
                if let Some(custom) = crate::settings::get_openclaw_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Hermes => {
                if let Some(custom) = crate::settings::get_hermes_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Pi => {}
        }

        // 默认路径：回退到用户主目录下的标准位置
        let home = dirs::home_dir().context(format_skill_error(
            "GET_HOME_DIR_FAILED",
            &[],
            Some("checkPermission"),
        ))?;

        Ok(match app {
            AppType::Claude => home.join(".claude").join("skills"),
            AppType::ClaudeDesktop => home.join(".claude-desktop").join("skills"),
            AppType::Codex => home.join(".agents").join("skills"),
            AppType::Gemini => home.join(".gemini").join("skills"),
            AppType::OpenCode => home.join(".config").join("opencode").join("skills"),
            AppType::OpenClaw => home.join(".openclaw").join("skills"),
            AppType::Hermes => crate::hermes_config::get_hermes_dir().join("skills"),
            AppType::Pi => crate::pi_config::get_pi_dir().join("skills"),
        })
    }

    // ========== 统一管理方法 ==========

    /// 获取所有已安装的 Skills
    pub fn get_all_installed(db: &Arc<Database>) -> Result<Vec<InstalledSkill>> {
        let skills = db.get_all_installed_skills()?;
        Ok(skills.into_values().collect())
    }

    pub(super) fn ensure_app_skill_support(app: &AppType) -> Result<()> {
        if app.capabilities().skills {
            return Ok(());
        }

        Err(anyhow!(
            "{} does not support CLI Skills management",
            app.as_str()
        ))
    }

    pub(super) fn app_scoped_skill_id(app: &AppType, id: &str) -> String {
        let prefix = format!("{}:", app.as_str());
        if id.starts_with(&prefix) {
            id.to_string()
        } else {
            format!("{prefix}{id}")
        }
    }

    pub(super) fn global_skill_id(id: &str) -> String {
        if id.starts_with("global:") {
            id.to_string()
        } else {
            format!("global:{id}")
        }
    }

    pub(super) fn is_app_scoped_skill_id(id: &str) -> bool {
        [
            "claude:",
            "codex:",
            "gemini:",
            "opencode:",
            "hermes:",
            "pi:",
        ]
        .iter()
        .any(|prefix| id.starts_with(prefix))
    }

    pub(super) fn find_global_record<'a, I>(
        records: I,
        directory: &str,
    ) -> Option<&'a InstalledSkill>
    where
        I: Iterator<Item = &'a InstalledSkill>,
    {
        records
            .filter(|skill| {
                skill.directory.eq_ignore_ascii_case(directory)
                    && (skill.id.starts_with("global:") || !Self::is_app_scoped_skill_id(&skill.id))
            })
            .max_by_key(|skill| skill.id.starts_with("global:"))
    }

    pub(super) fn app_skill_from_path(
        app: &AppType,
        path: &Path,
        directory: &str,
        metadata: Option<&InstalledSkill>,
    ) -> AppSkill {
        let is_markdown_file = path.is_file();
        let skill_md = if is_markdown_file {
            path.to_path_buf()
        } else {
            path.join("SKILL.md")
        };
        let (disk_name, disk_description) = Self::read_skill_name_desc(&skill_md, directory);
        let is_symlink = Self::is_symlink(path);
        let link_target =
            Self::resolved_symlink_target(path).map(|target| target.to_string_lossy().to_string());
        let global_source_path = Self::global_skills_path()
            .ok()
            .map(|root| root.join(directory));
        let global_source = !is_markdown_file
            && global_source_path
                .as_ref()
                .is_some_and(|source| source == path);
        let managed_globally = !is_markdown_file
            && global_source_path
                .and_then(|source| source.canonicalize().ok())
                .zip(path.canonicalize().ok())
                .is_some_and(|(source, resolved)| source == resolved);
        let installed_at = metadata.map(|skill| skill.installed_at).unwrap_or_else(|| {
            fs::metadata(path)
                .and_then(|value| value.modified())
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_secs() as i64)
                .unwrap_or_default()
        });

        AppSkill {
            id: metadata
                .map(|skill| skill.id.clone())
                .unwrap_or_else(|| format!("{}:local:{directory}", app.as_str())),
            name: if disk_name.trim().is_empty() {
                metadata
                    .map(|skill| skill.name.clone())
                    .unwrap_or_else(|| directory.to_string())
            } else {
                disk_name
            },
            description: disk_description
                .or_else(|| metadata.and_then(|skill| skill.description.clone())),
            directory: directory.to_string(),
            path: path.to_string_lossy().to_string(),
            is_symlink,
            link_target,
            managed_globally,
            global_source,
            repo_owner: metadata.and_then(|skill| skill.repo_owner.clone()),
            repo_name: metadata.and_then(|skill| skill.repo_name.clone()),
            repo_branch: metadata.and_then(|skill| skill.repo_branch.clone()),
            readme_url: metadata.and_then(|skill| skill.readme_url.clone()),
            installed_at,
            content_hash: metadata.and_then(|skill| skill.content_hash.clone()),
            updated_at: metadata.map(|skill| skill.updated_at).unwrap_or_default(),
        }
    }

    pub(super) fn collect_app_skill_dirs(
        root: &Path,
        current: &Path,
        depth: usize,
        max_depth: usize,
        results: &mut Vec<(String, PathBuf)>,
    ) -> Result<()> {
        if depth > max_depth {
            return Ok(());
        }

        for entry in fs::read_dir(current)
            .with_context(|| format!("读取 Skills 目录失败: {}", current.display()))?
        {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.join("SKILL.md").is_file() {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                results.push((relative, path));
                continue;
            }
            Self::collect_app_skill_dirs(root, &path, depth + 1, max_depth, results)?;
        }
        Ok(())
    }

    /// 直接读取指定 CLI 的原生 Skills 目录。
    ///
    /// 数据库只用于补充仓库信息，不决定技能是否存在。
    pub fn get_for_app(db: &Arc<Database>, app: &AppType) -> Result<AppSkillsResponse> {
        Self::ensure_app_skill_support(app)?;

        let skills_dir = Self::get_app_skills_dir(app)?;
        let metadata = db.get_all_installed_skills()?;
        let scoped_prefix = format!("{}:", app.as_str());
        let mut skills = Vec::new();

        if skills_dir.exists() {
            let max_depth = match app {
                AppType::OpenCode | AppType::Hermes | AppType::Pi => 8,
                AppType::Claude | AppType::Codex | AppType::Gemini => 1,
                AppType::ClaudeDesktop | AppType::OpenClaw => 0,
            };
            let mut native_skills = Vec::new();
            Self::collect_app_skill_dirs(
                &skills_dir,
                &skills_dir,
                1,
                max_depth,
                &mut native_skills,
            )?;
            if matches!(app, AppType::Pi) {
                for entry in fs::read_dir(&skills_dir)
                    .with_context(|| format!("读取 Pi Skills 目录失败: {}", skills_dir.display()))?
                {
                    let entry = entry?;
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.')
                        || !path.is_file()
                        || !name.to_ascii_lowercase().ends_with(".md")
                    {
                        continue;
                    }
                    native_skills.push((name, path));
                }
            }

            for (directory, path) in native_skills {
                let entry_metadata = metadata
                    .values()
                    .filter(|skill| {
                        skill.apps.is_enabled_for(app)
                            && skill.directory.eq_ignore_ascii_case(&directory)
                    })
                    .max_by_key(|skill| skill.id.starts_with(&scoped_prefix));

                skills.push(Self::app_skill_from_path(
                    app,
                    &path,
                    &directory,
                    entry_metadata,
                ));
            }
        }

        skills.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.directory.cmp(&right.directory))
        });

        Ok(AppSkillsResponse {
            app: app.as_str().to_string(),
            skills_dir: skills_dir.to_string_lossy().to_string(),
            skills,
        })
    }

    pub(super) fn paths_are_same(left: &Path, right: &Path) -> bool {
        if left == right {
            return true;
        }
        match (left.canonicalize(), right.canonicalize()) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
    }

    /// 该 CLI 的主安装目录是否就是全局 Skills 目录。
    ///
    /// 这决定安装、卸载和备份应落在哪个目录，不能与“是否额外扫描全局目录”混用。
    pub(super) fn app_primary_skills_dir_is_global(app: &AppType) -> bool {
        Self::get_app_skills_dir(app)
            .ok()
            .zip(Self::global_skills_path().ok())
            .is_some_and(|(app_dir, global_dir)| Self::paths_are_same(&app_dir, &global_dir))
    }

    pub(super) fn expand_external_skill_dir(raw: &str) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let expanded_home = if raw == "~" {
            home.to_string_lossy().to_string()
        } else if let Some(relative) = raw.strip_prefix("~/") {
            home.join(relative).to_string_lossy().to_string()
        } else {
            raw.to_string()
        };
        let variable_pattern = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").ok()?;
        let expanded = variable_pattern
            .replace_all(&expanded_home, |captures: &regex::Captures<'_>| {
                std::env::var(&captures[1]).unwrap_or_else(|_| captures[0].to_string())
            })
            .into_owned();
        let path = PathBuf::from(expanded);
        path.is_absolute().then_some(path)
    }

    pub(super) fn hermes_reads_global_skills_dir(global_dir: &Path) -> bool {
        let Ok(config) = crate::hermes_config::read_hermes_config() else {
            return false;
        };
        config
            .get("skills")
            .and_then(|skills| skills.get("external_dirs"))
            .and_then(serde_yaml::Value::as_sequence)
            .into_iter()
            .flatten()
            .filter_map(serde_yaml::Value::as_str)
            .filter_map(Self::expand_external_skill_dir)
            .any(|directory| Self::paths_are_same(&directory, global_dir))
    }

    /// 该 CLI 是否会把 `~/.agents/skills` 作为任一原生发现目录。
    ///
    /// Codex、Gemini CLI、OpenCode 和 Pi 原生支持 Agent Skills 兼容目录；Hermes 仅在
    /// `skills.external_dirs` 显式配置后扫描它。Claude Code 仍通过 `~/.claude/skills`
    /// 中的链接使用全局 Skill。
    pub(super) fn app_reads_global_skills_dir(app: &AppType) -> bool {
        if Self::app_primary_skills_dir_is_global(app) {
            return true;
        }
        match app {
            AppType::Codex | AppType::Gemini | AppType::OpenCode | AppType::Pi => true,
            AppType::Hermes => Self::global_skills_path()
                .ok()
                .is_some_and(|global_dir| Self::hermes_reads_global_skills_dir(&global_dir)),
            AppType::Claude | AppType::ClaudeDesktop | AppType::OpenClaw => false,
        }
    }

    pub(super) fn global_direct_apps() -> SkillApps {
        let mut apps = SkillApps::default();
        for app in [
            AppType::Claude,
            AppType::Codex,
            AppType::Gemini,
            AppType::OpenCode,
            AppType::Hermes,
            AppType::Pi,
        ] {
            apps.set_enabled_for(&app, Self::app_reads_global_skills_dir(&app));
        }
        apps
    }

    pub(super) fn global_link_states(source: &Path, directory: &str) -> SkillApps {
        let mut apps = SkillApps::default();
        for app in [
            AppType::Claude,
            AppType::Codex,
            AppType::Gemini,
            AppType::OpenCode,
            AppType::Hermes,
            AppType::Pi,
        ] {
            let available = if Self::app_reads_global_skills_dir(&app) {
                source.join("SKILL.md").is_file()
            } else {
                Self::get_app_skills_dir(&app)
                    .map(|dir| dir.join(directory))
                    .is_ok_and(|dest| Self::symlink_points_to(&dest, source))
            };
            apps.set_enabled_for(&app, available);
        }
        apps
    }

    pub(super) fn global_skill_from_path(
        path: &Path,
        directory: &str,
        metadata: Option<&InstalledSkill>,
    ) -> GlobalSkill {
        let (disk_name, disk_description) =
            Self::read_skill_name_desc(&path.join("SKILL.md"), directory);
        let installed_at = metadata.map(|skill| skill.installed_at).unwrap_or_else(|| {
            fs::metadata(path)
                .and_then(|value| value.modified())
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_secs() as i64)
                .unwrap_or_default()
        });

        GlobalSkill {
            id: metadata
                .map(|skill| skill.id.clone())
                .unwrap_or_else(|| format!("global:local:{directory}")),
            name: if disk_name.trim().is_empty() {
                metadata
                    .map(|skill| skill.name.clone())
                    .unwrap_or_else(|| directory.to_string())
            } else {
                disk_name
            },
            description: disk_description
                .or_else(|| metadata.and_then(|skill| skill.description.clone())),
            directory: directory.to_string(),
            path: path.to_string_lossy().to_string(),
            repo_owner: metadata.and_then(|skill| skill.repo_owner.clone()),
            repo_name: metadata.and_then(|skill| skill.repo_name.clone()),
            repo_branch: metadata.and_then(|skill| skill.repo_branch.clone()),
            readme_url: metadata.and_then(|skill| skill.readme_url.clone()),
            apps: Self::global_link_states(path, directory),
            installed_at,
            content_hash: metadata.and_then(|skill| skill.content_hash.clone()),
            updated_at: metadata.map(|skill| skill.updated_at).unwrap_or_default(),
        }
    }

    /// 读取 skills.sh 全局 Skills 目录及其对各 CLI 的实际可用状态。
    pub fn get_global(db: &Arc<Database>) -> Result<GlobalSkillsResponse> {
        let skills_dir = Self::get_global_skills_dir()?;
        let records = db.get_all_installed_skills()?;
        let mut native_skills = Vec::new();
        let mut skills = Vec::new();

        Self::collect_app_skill_dirs(&skills_dir, &skills_dir, 1, 1, &mut native_skills)?;
        for (directory, path) in native_skills {
            let metadata = Self::find_global_record(records.values(), &directory);
            skills.push(Self::global_skill_from_path(&path, &directory, metadata));
        }

        skills.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.directory.cmp(&right.directory))
        });

        Ok(GlobalSkillsResponse {
            skills_dir: skills_dir.to_string_lossy().to_string(),
            direct_apps: Self::global_direct_apps(),
            skills,
        })
    }

    /// 将全局 Skill 启用到指定 CLI。原生读取者无需链接，其他 CLI 创建或移除链接。
    ///
    /// 同名本地目录和指向其他位置的链接都不会被覆盖。
    pub fn set_global_link(
        db: &Arc<Database>,
        directory: &str,
        app: &AppType,
        enabled: bool,
    ) -> Result<GlobalSkill> {
        Self::ensure_app_skill_support(app)?;
        let relative = Self::sanitize_skill_source_path(directory)
            .ok_or_else(|| anyhow!("Invalid skill directory: {directory}"))?;
        let directory = relative.to_string_lossy().replace('\\', "/");
        let global_dir = Self::get_global_skills_dir()?;
        let source = global_dir.join(&relative);
        Self::validate_sync_source_dir(&source, &directory)?;

        let app_dir = Self::get_app_skills_dir(app)?;
        let dest = app_dir.join(&relative);
        if Self::app_reads_global_skills_dir(app) {
            if !enabled {
                return Err(anyhow!(
                    "{} reads the global Skills directory directly; uninstall the Skill from global management instead",
                    app.as_str()
                ));
            }
        } else if enabled {
            if Self::symlink_points_to(&dest, &source) {
                // 已经是正确的全局链接，无需重建。
            } else if dest.exists() || Self::is_symlink(&dest) {
                return Err(anyhow!(
                    "Cannot link global Skill '{}': {} already exists in {}",
                    directory,
                    dest.display(),
                    app.as_str()
                ));
            } else {
                let parent = dest
                    .parent()
                    .ok_or_else(|| anyhow!("Invalid skill destination: {}", dest.display()))?;
                fs::create_dir_all(parent)?;
                Self::create_symlink(&source, &dest)?;
            }
        } else if Self::symlink_points_to(&dest, &source) {
            Self::remove_path(&dest)?;
        }

        let records = db.get_all_installed_skills()?;
        let existing = Self::find_global_record(records.values(), &directory).cloned();
        let (disk_name, disk_description) =
            Self::read_skill_name_desc(&source.join("SKILL.md"), &directory);
        let mut record = existing.clone().unwrap_or_else(|| InstalledSkill {
            id: format!("global:local:{directory}"),
            name: disk_name,
            description: disk_description,
            directory: directory.clone(),
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            apps: SkillApps::default(),
            installed_at: Utc::now().timestamp(),
            content_hash: Self::compute_dir_hash(&source).ok(),
            updated_at: 0,
        });
        let old_id = record.id.clone();
        record.id = Self::global_skill_id(&record.id);
        record.apps = Self::global_link_states(&source, &directory);
        db.save_skill(&record)?;
        if old_id != record.id {
            let _ = db.delete_skill(&old_id);
        }

        Ok(Self::global_skill_from_path(
            &source,
            &directory,
            Some(&record),
        ))
    }
}
