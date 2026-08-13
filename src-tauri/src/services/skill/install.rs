use super::*;

impl SkillService {
    /// 将仓库中的 Skill 安装到全局目录。
    ///
    /// 直接读取 ~/.agents/skills 的 CLI 会立即可见；不会额外创建其他 CLI 链接。
    pub async fn install_global(
        &self,
        db: &Arc<Database>,
        skill: &DiscoverableSkill,
    ) -> Result<GlobalSkill> {
        let source_rel = Self::sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            anyhow!(format_skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            ))
        })?;
        let install_name = source_rel
            .file_name()
            .and_then(|name| Self::sanitize_install_name(&name.to_string_lossy()))
            .ok_or_else(|| {
                anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                ))
            })?;

        let global_dir = Self::get_global_skills_dir()?;
        let dest = global_dir.join(&install_name);
        if dest.exists() || Self::is_symlink(&dest) {
            return Err(anyhow!(format_skill_error(
                "SKILL_DIRECTORY_CONFLICT",
                &[
                    ("directory", &install_name),
                    ("existing_repo", "global library"),
                    (
                        "new_repo",
                        &format!("{}/{}", skill.repo_owner, skill.repo_name)
                    ),
                ],
                Some("uninstallFirst"),
            )));
        }

        let command_home = dirs::home_dir().context(format_skill_error(
            "GET_HOME_DIR_FAILED",
            &[],
            Some("checkPermission"),
        ))?;
        let repo_branch = self
            .install_remote_command_first(skill, &command_home, &dest, &install_name, true)
            .await?;

        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .map(|path| {
                if path.ends_with("/SKILL.md") || path == "SKILL.md" {
                    path
                } else {
                    format!("{}/SKILL.md", path.trim_end_matches('/'))
                }
            })
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let (disk_name, disk_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &install_name);
        let record = InstalledSkill {
            id: Self::global_skill_id(&skill.key),
            name: if disk_name.trim().is_empty() {
                skill.name.clone()
            } else {
                disk_name
            },
            description: disk_description
                .or_else(|| (!skill.description.is_empty()).then(|| skill.description.clone())),
            directory: install_name.clone(),
            repo_owner: Some(skill.repo_owner.clone()),
            repo_name: Some(skill.repo_name.clone()),
            repo_branch: Some(repo_branch.clone()),
            readme_url: Some(Self::build_skill_doc_url(
                &skill.repo_owner,
                &skill.repo_name,
                &repo_branch,
                &doc_path,
            )),
            apps: Self::global_link_states(&dest, &install_name),
            installed_at: Utc::now().timestamp(),
            content_hash: Self::compute_dir_hash(&dest).ok(),
            updated_at: 0,
        };
        if let Err(error) = db.save_skill(&record) {
            let _ = Self::remove_path(&dest);
            return Err(error.into());
        }
        Ok(Self::global_skill_from_path(
            &dest,
            &install_name,
            Some(&record),
        ))
    }

    /// 将仓库中的 Skill 直接安装到指定 CLI 的原生目录。
    pub async fn install_for_app(
        &self,
        db: &Arc<Database>,
        skill: &DiscoverableSkill,
        app: &AppType,
    ) -> Result<AppSkill> {
        Self::ensure_app_skill_support(app)?;
        if Self::app_primary_skills_dir_is_global(app) {
            let installed = self.install_global(db, skill).await?;
            let path = Self::get_global_skills_dir()?.join(&installed.directory);
            let records = db.get_all_installed_skills()?;
            let metadata = Self::find_global_record(records.values(), &installed.directory);
            return Ok(Self::app_skill_from_path(
                app,
                &path,
                &installed.directory,
                metadata,
            ));
        }

        let source_rel = Self::sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            anyhow!(format_skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            ))
        })?;
        let install_name = source_rel
            .file_name()
            .and_then(|name| Self::sanitize_install_name(&name.to_string_lossy()))
            .ok_or_else(|| {
                anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                ))
            })?;

        let app_dir = Self::get_app_skills_dir(app)?;
        fs::create_dir_all(&app_dir)?;
        let dest = app_dir.join(&install_name);
        if dest.exists() || Self::is_symlink(&dest) {
            return Err(anyhow!(format_skill_error(
                "SKILL_DIRECTORY_CONFLICT",
                &[
                    ("directory", &install_name),
                    ("existing_repo", "current CLI"),
                    (
                        "new_repo",
                        &format!("{}/{}", skill.repo_owner, skill.repo_name)
                    ),
                ],
                Some("uninstallFirst"),
            )));
        }

        let command_home = Self::skills_cli_workspace_for_app(app);
        let repo_branch = self
            .install_remote_command_first(skill, &command_home, &dest, &install_name, false)
            .await?;

        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .map(|path| {
                if path.ends_with("/SKILL.md") || path == "SKILL.md" {
                    path
                } else {
                    format!("{}/SKILL.md", path.trim_end_matches('/'))
                }
            })
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let (disk_name, disk_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &install_name);
        let installed_skill = InstalledSkill {
            id: Self::app_scoped_skill_id(app, &skill.key),
            name: if disk_name.trim().is_empty() {
                skill.name.clone()
            } else {
                disk_name
            },
            description: disk_description
                .or_else(|| (!skill.description.is_empty()).then(|| skill.description.clone())),
            directory: install_name.clone(),
            repo_owner: Some(skill.repo_owner.clone()),
            repo_name: Some(skill.repo_name.clone()),
            repo_branch: Some(repo_branch.clone()),
            readme_url: Some(Self::build_skill_doc_url(
                &skill.repo_owner,
                &skill.repo_name,
                &repo_branch,
                &doc_path,
            )),
            apps: SkillApps::only(app),
            installed_at: Utc::now().timestamp(),
            content_hash: Self::compute_dir_hash(&dest).ok(),
            updated_at: 0,
        };

        if let Err(error) = db.save_skill(&installed_skill) {
            let _ = Self::remove_path(&dest);
            return Err(error.into());
        }

        Ok(Self::app_skill_from_path(
            app,
            &dest,
            &install_name,
            Some(&installed_skill),
        ))
    }

    /// 安装 Skill
    ///
    /// 流程：
    /// 1. 下载到 SSOT 目录
    /// 2. 保存到数据库
    /// 3. 同步到启用的应用目录
    pub async fn install(
        &self,
        db: &Arc<Database>,
        skill: &DiscoverableSkill,
        current_app: &AppType,
    ) -> Result<InstalledSkill> {
        let ssot_dir = Self::get_ssot_dir()?;

        // 允许多级目录（如 a/b/c），但必须是安全的相对路径。
        let source_rel = Self::sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            anyhow!(format_skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            ))
        })?;
        // 安装目录名始终使用最后一段，避免在 SSOT 中创建多级目录。
        let install_name = source_rel
            .file_name()
            .and_then(|name| Self::sanitize_install_name(&name.to_string_lossy()))
            .ok_or_else(|| {
                anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                ))
            })?;

        // 检查数据库中是否已有同名 directory 的 skill（来自其他仓库）
        let existing_skills = db.get_all_installed_skills()?;
        for existing in existing_skills.values() {
            if existing.directory.eq_ignore_ascii_case(&install_name) {
                // 检查是否来自同一仓库
                let same_repo = existing.repo_owner.as_deref() == Some(&skill.repo_owner)
                    && existing.repo_name.as_deref() == Some(&skill.repo_name);
                if same_repo {
                    // 同一仓库的同名 skill，返回现有记录（可能需要更新启用状态）
                    let mut updated = existing.clone();
                    updated.apps.set_enabled_for(current_app, true);
                    db.save_skill(&updated)?;
                    Self::sync_to_app_dir(&updated.directory, current_app)?;
                    log::info!(
                        "Skill {} 已存在，更新 {:?} 启用状态",
                        updated.name,
                        current_app
                    );
                    return Ok(updated);
                } else {
                    // 不同仓库的同名 skill，报错
                    return Err(anyhow!(format_skill_error(
                        "SKILL_DIRECTORY_CONFLICT",
                        &[
                            ("directory", &install_name),
                            (
                                "existing_repo",
                                &format!(
                                    "{}/{}",
                                    existing.repo_owner.as_deref().unwrap_or("unknown"),
                                    existing.repo_name.as_deref().unwrap_or("unknown")
                                )
                            ),
                            (
                                "new_repo",
                                &format!("{}/{}", skill.repo_owner, skill.repo_name)
                            ),
                        ],
                        Some("uninstallFirst"),
                    )));
                }
            }
        }

        let dest = ssot_dir.join(&install_name);

        let mut repo_branch = skill.repo_branch.clone();

        // 如果已存在则跳过下载
        if !dest.exists() {
            let global_path = Self::global_skills_path()?;
            let global_command = Self::paths_are_same(&ssot_dir, &global_path);
            let command_home = if global_command {
                dirs::home_dir().context(format_skill_error(
                    "GET_HOME_DIR_FAILED",
                    &[],
                    Some("checkPermission"),
                ))?
            } else {
                Self::skills_cli_workspace_for_unified()
            };
            repo_branch = self
                .install_remote_command_first(
                    skill,
                    &command_home,
                    &dest,
                    &install_name,
                    global_command,
                )
                .await?;

            // 使用实际下载成功的分支，避免 readme_url / repo_branch 与真实分支不一致。
            if repo_branch != skill.repo_branch {
                log::info!(
                    "Skill {}/{} 分支自动回退: {} -> {}",
                    skill.repo_owner,
                    skill.repo_name,
                    skill.repo_branch,
                    repo_branch
                );
            }
        }

        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .map(|path| {
                if path.ends_with("/SKILL.md") || path == "SKILL.md" {
                    path
                } else {
                    format!("{}/SKILL.md", path.trim_end_matches('/'))
                }
            })
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));

        let readme_url = Some(Self::build_skill_doc_url(
            &skill.repo_owner,
            &skill.repo_name,
            &repo_branch,
            &doc_path,
        ));

        // 创建 InstalledSkill 记录
        // 计算内容哈希
        let content_hash = Self::compute_dir_hash(&dest).map(Some).unwrap_or_else(|e| {
            log::warn!("Failed to compute content hash for {}: {e}", install_name);
            None
        });

        let installed_skill = InstalledSkill {
            id: skill.key.clone(),
            name: skill.name.clone(),
            description: if skill.description.is_empty() {
                None
            } else {
                Some(skill.description.clone())
            },
            directory: install_name.clone(),
            repo_owner: Some(skill.repo_owner.clone()),
            repo_name: Some(skill.repo_name.clone()),
            repo_branch: Some(repo_branch),
            readme_url,
            apps: SkillApps::only(current_app),
            installed_at: chrono::Utc::now().timestamp(),
            content_hash,
            updated_at: 0,
        };

        // 保存到数据库
        db.save_skill(&installed_skill)?;

        // 同步到当前应用目录
        Self::sync_to_app_dir(&install_name, current_app)?;

        log::info!(
            "Skill {} 安装成功，已启用 {:?}",
            installed_skill.name,
            current_app
        );

        Ok(installed_skill)
    }

    /// 卸载 Skill
    ///
    /// 流程：
    /// 1. 从所有应用目录删除
    /// 2. 从 SSOT 删除
    /// 3. 从数据库删除
    pub fn uninstall(db: &Arc<Database>, id: &str) -> Result<SkillUninstallResult> {
        // 获取 skill 信息
        let skill = db
            .get_installed_skill(id)?
            .ok_or_else(|| anyhow!("Skill not found: {id}"))?;

        let backup_path =
            Self::create_uninstall_backup(&skill)?.map(|path| path.to_string_lossy().to_string());

        // 从所有应用目录删除
        for app in AppType::all() {
            let _ = Self::remove_from_app(&skill.directory, &app);
        }

        // 从 SSOT 删除
        let ssot_dir = Self::get_ssot_dir()?;
        let skill_path = ssot_dir.join(&skill.directory);
        if skill_path.exists() {
            fs::remove_dir_all(&skill_path)?;
        }

        // 从数据库删除
        db.delete_skill(id)?;

        log::info!(
            "Skill {} 卸载成功{}",
            skill.name,
            backup_path
                .as_deref()
                .map(|path| format!(", backup: {path}"))
                .unwrap_or_default()
        );

        Ok(SkillUninstallResult { backup_path })
    }

    /// 仅从指定 CLI 的原生目录卸载 Skill。
    pub fn uninstall_for_app(
        db: &Arc<Database>,
        app: &AppType,
        directory: &str,
    ) -> Result<SkillUninstallResult> {
        Self::ensure_app_skill_support(app)?;
        let relative = Self::sanitize_skill_source_path(directory)
            .ok_or_else(|| anyhow!("Invalid skill directory: {directory}"))?;
        let directory = relative.to_string_lossy().replace('\\', "/");
        let app_dir = Self::get_app_skills_dir(app)?;
        let path = app_dir.join(&relative);
        let is_pi_markdown_file = matches!(app, AppType::Pi)
            && relative.components().count() == 1
            && path.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("md"));
        if (!path.exists() && !Self::is_symlink(&path))
            || (!is_pi_markdown_file && !path.join("SKILL.md").is_file())
        {
            return Err(anyhow!(
                "Skill not found in {}: {}",
                app.as_str(),
                directory
            ));
        }
        let global_source = Self::get_global_skills_dir()?.join(&relative);
        if path == global_source {
            return Err(anyhow!(
                "{} reads this Skill directly from the global directory; uninstall it from global management instead",
                app.as_str()
            ));
        }

        let records = db.get_all_installed_skills()?;
        let scoped_prefix = format!("{}:", app.as_str());
        let selected = records
            .values()
            .filter(|skill| {
                skill.apps.is_enabled_for(app) && skill.directory.eq_ignore_ascii_case(&directory)
            })
            .max_by_key(|skill| skill.id.starts_with(&scoped_prefix))
            .cloned();
        let skill = selected.unwrap_or_else(|| {
            let manifest = if is_pi_markdown_file {
                path.clone()
            } else {
                path.join("SKILL.md")
            };
            let (name, description) = Self::read_skill_name_desc(&manifest, &directory);
            InstalledSkill {
                id: format!("{}:local:{directory}", app.as_str()),
                name,
                description,
                directory: directory.clone(),
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                apps: SkillApps::only(app),
                installed_at: Utc::now().timestamp(),
                content_hash: if is_pi_markdown_file {
                    None
                } else {
                    Self::compute_dir_hash(&path).ok()
                },
                updated_at: 0,
            }
        });

        let backup_path = Self::create_backup_from_source(&skill, &path, Some(app.clone()))?
            .map(|value| value.to_string_lossy().to_string());
        Self::remove_path(&path)?;

        for mut record in records.into_values().filter(|record| {
            record.apps.is_enabled_for(app) && record.directory.eq_ignore_ascii_case(&directory)
        }) {
            record.apps.set_enabled_for(app, false);
            if record.apps.is_empty() {
                db.delete_skill(&record.id)?;
            } else {
                db.save_skill(&record)?;
            }
        }

        Ok(SkillUninstallResult { backup_path })
    }

    /// 从全局库卸载 Skill，并移除所有由它创建的 CLI 软链接。
    pub fn uninstall_global(db: &Arc<Database>, directory: &str) -> Result<SkillUninstallResult> {
        let relative = Self::sanitize_skill_source_path(directory)
            .ok_or_else(|| anyhow!("Invalid skill directory: {directory}"))?;
        let directory = relative.to_string_lossy().replace('\\', "/");
        let global_dir = Self::get_global_skills_dir()?;
        let source = global_dir.join(&relative);
        if !source.is_dir() || !source.join("SKILL.md").is_file() {
            return Err(anyhow!("Global Skill not found: {directory}"));
        }

        let records = db.get_all_installed_skills()?;
        let selected = Self::find_global_record(records.values(), &directory).cloned();
        let skill = selected.unwrap_or_else(|| {
            let (name, description) =
                Self::read_skill_name_desc(&source.join("SKILL.md"), &directory);
            InstalledSkill {
                id: format!("global:local:{directory}"),
                name,
                description,
                directory: directory.clone(),
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                apps: Self::global_link_states(&source, &directory),
                installed_at: Utc::now().timestamp(),
                content_hash: Self::compute_dir_hash(&source).ok(),
                updated_at: 0,
            }
        });
        let backup_path = Self::create_backup_from_source(&skill, &source, None)?
            .map(|path| path.to_string_lossy().to_string());

        for app in [
            AppType::Claude,
            AppType::Codex,
            AppType::Gemini,
            AppType::OpenCode,
            AppType::Hermes,
            AppType::Pi,
        ] {
            let dest = Self::get_app_skills_dir(&app)?.join(&relative);
            if Self::symlink_points_to(&dest, &source) {
                Self::remove_path(&dest)?;
            }
        }
        Self::remove_path(&source)?;

        for record in records.values().filter(|record| {
            record.directory.eq_ignore_ascii_case(&directory)
                && (record.id.starts_with("global:") || !Self::is_app_scoped_skill_id(&record.id))
        }) {
            db.delete_skill(&record.id)?;
        }

        Ok(SkillUninstallResult { backup_path })
    }

    pub fn list_global_backups() -> Result<Vec<SkillBackupEntry>> {
        Ok(Self::list_backups()?
            .into_iter()
            .filter(|entry| entry.source_app.is_none())
            .collect())
    }

    /// 将备份恢复到全局目录；恢复后不会自动创建额外的 CLI 链接。
    pub fn restore_global(db: &Arc<Database>, backup_id: &str) -> Result<GlobalSkill> {
        let backup_path = Self::backup_path_for_id(backup_id)?;
        let metadata = Self::read_backup_metadata(&backup_path)?;
        if metadata.source_app.is_some() {
            return Err(anyhow!("This backup belongs to a CLI Skill"));
        }

        let source = backup_path.join("skill");
        if !source.join("SKILL.md").is_file() {
            return Err(anyhow!(
                "Skill backup is invalid or missing SKILL.md: {}",
                backup_path.display()
            ));
        }
        let relative = Self::sanitize_skill_source_path(&metadata.skill.directory)
            .ok_or_else(|| anyhow!("Invalid skill directory: {}", metadata.skill.directory))?;
        let install_name = relative
            .file_name()
            .and_then(|name| Self::sanitize_install_name(&name.to_string_lossy()))
            .ok_or_else(|| anyhow!("Invalid skill directory: {}", metadata.skill.directory))?;
        let global_dir = Self::get_global_skills_dir()?;
        let dest = global_dir.join(&install_name);
        Self::copy_skill_to_new_dest(&source, &dest, &install_name)?;

        let mut restored = metadata.skill;
        let old_id = restored.id.clone();
        restored.id = Self::global_skill_id(&restored.id);
        restored.directory = install_name.clone();
        restored.apps = Self::global_link_states(&dest, &install_name);
        restored.installed_at = Utc::now().timestamp();
        restored.updated_at = 0;
        restored.content_hash = Self::compute_dir_hash(&dest).ok();
        if let Err(error) = db.save_skill(&restored) {
            let _ = Self::remove_path(&dest);
            return Err(error.into());
        }
        if old_id != restored.id {
            let _ = db.delete_skill(&old_id);
        }

        Ok(Self::global_skill_from_path(
            &dest,
            &install_name,
            Some(&restored),
        ))
    }
}
