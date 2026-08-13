use super::*;

impl SkillService {
    // ========== 更新检测 ==========

    /// 计算目录内容的 SHA-256 哈希
    ///
    /// 递归遍历目录下所有非隐藏文件，按相对路径字典序排列，
    /// 将 "相对路径\0内容\0" 逐文件 feed 给同一个 hasher。
    pub fn compute_dir_hash(dir: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let mut files: Vec<PathBuf> = Vec::new();
        Self::collect_files_for_hash(dir, dir, &mut files)?;
        files.sort();

        let mut hasher = Sha256::new();
        for file_path in &files {
            let relative = file_path.strip_prefix(dir).unwrap_or(file_path);
            let rel_str = relative.to_string_lossy().replace('\\', "/");
            hasher.update(rel_str.as_bytes());
            hasher.update(b"\0");
            let content = fs::read(file_path)
                .with_context(|| format!("读取文件失败: {}", file_path.display()))?;
            hasher.update(&content);
            hasher.update(b"\0");
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// 递归收集目录下所有非隐藏文件
    #[allow(clippy::only_used_in_recursion)]
    pub(super) fn collect_files_for_hash(
        base: &Path,
        current: &Path,
        files: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let entries = fs::read_dir(current)
            .with_context(|| format!("读取目录失败: {}", current.display()))?;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                Self::collect_files_for_hash(base, &path, files)?;
            } else {
                files.push(path);
            }
        }
        Ok(())
    }

    /// 检查所有已安装 Skill 的更新
    ///
    /// 仅检查有 repo_owner 的 Skill（本地 Skill 跳过），
    /// 按仓库分组下载，避免重复下载同一仓库。
    pub async fn check_updates(&self, db: &Arc<Database>) -> Result<Vec<SkillUpdateInfo>> {
        let skills = db.get_all_installed_skills()?;
        let mut updates = Vec::new();

        // 按 (owner, name, branch) 分组
        let mut repo_groups: HashMap<(String, String, String), Vec<InstalledSkill>> =
            HashMap::new();

        for skill in skills.into_values() {
            let (owner, name, branch) =
                match (&skill.repo_owner, &skill.repo_name, &skill.repo_branch) {
                    (Some(o), Some(n), Some(b)) => (o.clone(), n.clone(), b.clone()),
                    (Some(o), Some(n), None) => (o.clone(), n.clone(), "main".to_string()),
                    _ => continue,
                };
            repo_groups
                .entry((owner, name, branch))
                .or_default()
                .push(skill);
        }

        let ssot_dir = Self::get_ssot_dir()?;

        for ((owner, name, branch), group_skills) in &repo_groups {
            let repo = SkillRepo {
                owner: owner.clone(),
                name: name.clone(),
                branch: branch.clone(),
                enabled: true,
            };

            // 下载仓库 ZIP
            let (temp_dir, _used_branch) = match timeout(
                std::time::Duration::from_secs(60),
                self.download_repo(&repo),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    log::warn!("检查更新时下载 {}/{} 失败: {e}", owner, name);
                    continue;
                }
                Err(_) => {
                    log::warn!("检查更新时下载 {}/{} 超时", owner, name);
                    continue;
                }
            };

            // 扫描仓库中的所有 Skill 目录
            let mut remote_skills: Vec<DiscoverableSkill> = Vec::new();
            let _ = self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills);

            for skill in group_skills {
                // 在远程仓库中找到匹配的 Skill 目录
                let remote_match = remote_skills.iter().find(|rs| {
                    // 匹配方式：安装名称的最后一段
                    let remote_install_name =
                        rs.directory.rsplit('/').next().unwrap_or(&rs.directory);
                    remote_install_name.eq_ignore_ascii_case(&skill.directory)
                });

                let remote_skill_dir = match remote_match {
                    Some(rs) => match Self::resolve_skill_source_dir(&temp_dir, &rs.directory) {
                        Some(path) => path,
                        None => continue,
                    },
                    None => continue,
                };

                let remote_hash = match Self::compute_dir_hash(&remote_skill_dir) {
                    Ok(h) => h,
                    Err(e) => {
                        log::warn!("计算远程哈希失败 {}: {e}", skill.id);
                        continue;
                    }
                };

                // 本地哈希：优先数据库，否则实时计算
                let local_hash = match &skill.content_hash {
                    Some(h) => Some(h.clone()),
                    None => {
                        let local_dir = ssot_dir.join(&skill.directory);
                        if local_dir.exists() {
                            match Self::compute_dir_hash(&local_dir) {
                                Ok(h) => {
                                    let _ = db.update_skill_hash(&skill.id, &h, 0);
                                    Some(h)
                                }
                                Err(_) => None,
                            }
                        } else {
                            None
                        }
                    }
                };

                if local_hash.as_deref() != Some(&remote_hash) {
                    updates.push(SkillUpdateInfo {
                        id: skill.id.clone(),
                        name: skill.name.clone(),
                        current_hash: local_hash,
                        remote_hash,
                    });
                }
            }

            let _ = fs::remove_dir_all(&temp_dir);
        }

        Ok(updates)
    }

    /// 仅检查全局库中带仓库元数据的 Skills。
    pub async fn check_global_updates(&self, db: &Arc<Database>) -> Result<Vec<SkillUpdateInfo>> {
        let global = Self::get_global(db)?;
        let mut updates = Vec::new();
        let mut repo_groups: HashMap<(String, String, String), Vec<GlobalSkill>> = HashMap::new();

        for skill in global.skills {
            let (owner, name, branch) =
                match (&skill.repo_owner, &skill.repo_name, &skill.repo_branch) {
                    (Some(owner), Some(name), Some(branch)) => {
                        (owner.clone(), name.clone(), branch.clone())
                    }
                    (Some(owner), Some(name), None) => {
                        (owner.clone(), name.clone(), "main".to_string())
                    }
                    _ => continue,
                };
            repo_groups
                .entry((owner, name, branch))
                .or_default()
                .push(skill);
        }

        for ((owner, name, branch), skills) in repo_groups {
            let repo = SkillRepo {
                owner: owner.clone(),
                name: name.clone(),
                branch,
                enabled: true,
            };
            let (temp_dir, _) = match timeout(
                std::time::Duration::from_secs(60),
                self.download_repo(&repo),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => {
                    log::warn!("检查全局 Skill 更新时下载 {owner}/{name} 失败: {error}");
                    continue;
                }
                Err(_) => {
                    log::warn!("检查全局 Skill 更新时下载 {owner}/{name} 超时");
                    continue;
                }
            };

            let mut remote_skills = Vec::new();
            let _ = self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills);
            for skill in skills {
                let Some(remote) = remote_skills.iter().find(|candidate| {
                    candidate
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&candidate.directory)
                        .eq_ignore_ascii_case(&skill.directory)
                }) else {
                    continue;
                };
                let Some(remote_dir) = Self::resolve_skill_source_dir(&temp_dir, &remote.directory)
                else {
                    continue;
                };
                let Ok(remote_hash) = Self::compute_dir_hash(&remote_dir) else {
                    continue;
                };
                let current_hash = Self::compute_dir_hash(Path::new(&skill.path)).ok();
                if current_hash.as_deref() != Some(&remote_hash) {
                    updates.push(SkillUpdateInfo {
                        id: skill.id,
                        name: skill.name,
                        current_hash,
                        remote_hash,
                    });
                }
            }
            let _ = fs::remove_dir_all(&temp_dir);
        }

        Ok(updates)
    }

    /// 仅检查指定 CLI 原生目录中的仓库型 Skills；全局软链接由全局页更新。
    pub async fn check_app_updates(
        &self,
        db: &Arc<Database>,
        app: &AppType,
    ) -> Result<Vec<SkillUpdateInfo>> {
        Self::ensure_app_skill_support(app)?;
        let snapshot = Self::get_for_app(db, app)?;
        let mut updates = Vec::new();
        let mut repo_groups: HashMap<(String, String, String), Vec<AppSkill>> = HashMap::new();

        for skill in snapshot
            .skills
            .into_iter()
            .filter(|skill| !skill.managed_globally)
        {
            let (owner, name, branch) =
                match (&skill.repo_owner, &skill.repo_name, &skill.repo_branch) {
                    (Some(owner), Some(name), Some(branch)) => {
                        (owner.clone(), name.clone(), branch.clone())
                    }
                    (Some(owner), Some(name), None) => {
                        (owner.clone(), name.clone(), "main".to_string())
                    }
                    _ => continue,
                };
            repo_groups
                .entry((owner, name, branch))
                .or_default()
                .push(skill);
        }

        for ((owner, name, branch), skills) in repo_groups {
            let repo = SkillRepo {
                owner: owner.clone(),
                name: name.clone(),
                branch,
                enabled: true,
            };
            let (temp_dir, _) = match timeout(
                std::time::Duration::from_secs(60),
                self.download_repo(&repo),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => {
                    log::warn!(
                        "检查 {} Skill 更新时下载 {owner}/{name} 失败: {error}",
                        app.as_str()
                    );
                    continue;
                }
                Err(_) => {
                    log::warn!("检查 {} Skill 更新时下载 {owner}/{name} 超时", app.as_str());
                    continue;
                }
            };

            let mut remote_skills = Vec::new();
            let _ = self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills);
            for skill in skills {
                let install_name = skill
                    .directory
                    .rsplit('/')
                    .next()
                    .unwrap_or(&skill.directory);
                let Some(remote) = remote_skills.iter().find(|candidate| {
                    candidate
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&candidate.directory)
                        .eq_ignore_ascii_case(install_name)
                }) else {
                    continue;
                };
                let Some(remote_dir) = Self::resolve_skill_source_dir(&temp_dir, &remote.directory)
                else {
                    continue;
                };
                let Ok(remote_hash) = Self::compute_dir_hash(&remote_dir) else {
                    continue;
                };
                let current_hash = Self::compute_dir_hash(Path::new(&skill.path)).ok();
                if current_hash.as_deref() != Some(&remote_hash) {
                    updates.push(SkillUpdateInfo {
                        id: skill.id,
                        name: skill.name,
                        current_hash,
                        remote_hash,
                    });
                }
            }
            let _ = fs::remove_dir_all(&temp_dir);
        }

        Ok(updates)
    }

    pub(super) fn save_updated_global_skill(
        db: &Arc<Database>,
        skill: &InstalledSkill,
        owner: &str,
        repository: &str,
        branch: &str,
        dest: &Path,
    ) -> Result<GlobalSkill> {
        let (new_name, new_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &skill.directory);
        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let old_id = skill.id.clone();
        let updated = InstalledSkill {
            id: Self::global_skill_id(&skill.id),
            name: new_name,
            description: new_description,
            directory: skill.directory.clone(),
            repo_owner: skill.repo_owner.clone(),
            repo_name: skill.repo_name.clone(),
            repo_branch: Some(branch.to_string()),
            readme_url: Some(Self::build_skill_doc_url(
                owner, repository, branch, &doc_path,
            )),
            apps: Self::global_link_states(dest, &skill.directory),
            installed_at: skill.installed_at,
            content_hash: Self::compute_dir_hash(dest).ok(),
            updated_at: Utc::now().timestamp(),
        };
        db.save_skill(&updated)?;
        if old_id != updated.id {
            let _ = db.delete_skill(&old_id);
        }
        Ok(Self::global_skill_from_path(
            dest,
            &skill.directory,
            Some(&updated),
        ))
    }

    pub(super) fn save_updated_app_skill(
        db: &Arc<Database>,
        app: &AppType,
        app_skill: &AppSkill,
        skill_id: &str,
        repository: (&str, &str, &str),
        dest: &Path,
    ) -> Result<AppSkill> {
        let (owner, repository, branch) = repository;
        let records = db.get_all_installed_skills()?;
        let previous = records.get(skill_id).cloned();
        let (new_name, new_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &app_skill.directory);
        let doc_path = app_skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .unwrap_or_else(|| format!("{}/SKILL.md", app_skill.directory.trim_end_matches('/')));
        let updated = InstalledSkill {
            id: Self::app_scoped_skill_id(app, skill_id),
            name: new_name,
            description: new_description,
            directory: app_skill.directory.clone(),
            repo_owner: app_skill.repo_owner.clone(),
            repo_name: app_skill.repo_name.clone(),
            repo_branch: Some(branch.to_string()),
            readme_url: Some(Self::build_skill_doc_url(
                owner, repository, branch, &doc_path,
            )),
            apps: SkillApps::only(app),
            installed_at: app_skill.installed_at,
            content_hash: Self::compute_dir_hash(dest).ok(),
            updated_at: Utc::now().timestamp(),
        };
        db.save_skill(&updated)?;
        if let Some(mut previous) = previous.filter(|record| record.id != updated.id) {
            previous.apps.set_enabled_for(app, false);
            if previous.apps.is_empty() {
                let _ = db.delete_skill(&previous.id);
            } else {
                db.save_skill(&previous)?;
            }
        }
        Ok(Self::app_skill_from_path(
            app,
            dest,
            &app_skill.directory,
            Some(&updated),
        ))
    }

    pub(super) fn save_updated_unified_skill(
        db: &Arc<Database>,
        skill: &InstalledSkill,
        owner: &str,
        repository: &str,
        branch: &str,
        dest: &Path,
    ) -> Result<InstalledSkill> {
        let (new_name, new_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &skill.directory);
        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let updated = InstalledSkill {
            id: skill.id.clone(),
            name: new_name,
            description: new_description,
            directory: skill.directory.clone(),
            repo_owner: skill.repo_owner.clone(),
            repo_name: skill.repo_name.clone(),
            repo_branch: Some(branch.to_string()),
            readme_url: Some(Self::build_skill_doc_url(
                owner, repository, branch, &doc_path,
            )),
            apps: skill.apps.clone(),
            installed_at: skill.installed_at,
            content_hash: Self::compute_dir_hash(dest).ok(),
            updated_at: Utc::now().timestamp(),
        };

        db.save_skill(&updated)?;
        for app in updated.apps.enabled_apps() {
            if let Err(error) = Self::sync_to_app_dir(&updated.directory, &app) {
                log::warn!("同步更新后的 Skill 到 {:?} 失败: {error}", app);
            }
        }

        log::info!("Skill {} 更新成功", updated.name);
        Ok(updated)
    }

    /// 更新全局库中的单个 Skill；现有 CLI 软链接会继续指向同一路径。
    pub async fn update_global_skill(
        &self,
        db: &Arc<Database>,
        skill_id: &str,
    ) -> Result<GlobalSkill> {
        let records = db.get_all_installed_skills()?;
        let skill = records
            .get(skill_id)
            .filter(|skill| {
                skill.id.starts_with("global:") || !Self::is_app_scoped_skill_id(&skill.id)
            })
            .cloned()
            .ok_or_else(|| anyhow!("Global Skill not found: {skill_id}"))?;
        let (owner, name, branch) = match (&skill.repo_owner, &skill.repo_name) {
            (Some(owner), Some(name)) => (
                owner.clone(),
                name.clone(),
                skill
                    .repo_branch
                    .clone()
                    .unwrap_or_else(|| "main".to_string()),
            ),
            _ => return Err(anyhow!("Cannot update local Skill: {skill_id}")),
        };
        let repo = SkillRepo {
            owner: owner.clone(),
            name: name.clone(),
            branch: branch.clone(),
            enabled: true,
        };
        let global_dir = Self::get_global_skills_dir()?;
        let dest = global_dir.join(&skill.directory);
        Self::validate_sync_source_dir(&dest, &skill.directory)?;
        let _ = Self::create_backup_from_source(&skill, &dest, None)?;

        let command_home = dirs::home_dir().context(format_skill_error(
            "GET_HOME_DIR_FAILED",
            &[],
            Some("checkPermission"),
        ))?;
        match Self::try_update_remote_with_skills_cli(&command_home, &dest, &skill.directory, true)
            .await
        {
            Ok(recorded_branch) => {
                let updated_branch = recorded_branch.unwrap_or(branch);
                return Self::save_updated_global_skill(
                    db,
                    &skill,
                    &owner,
                    &name,
                    &updated_branch,
                    &dest,
                );
            }
            Err(command_error) => {
                log::warn!(
                    "skills CLI update failed for global Skill {}; falling back to archive: {command_error}",
                    skill.directory
                );
            }
        }

        let (temp_dir, used_branch) = timeout(
            std::time::Duration::from_secs(60),
            self.download_repo(&repo),
        )
        .await
        .map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_TIMEOUT",
                &[("owner", &owner), ("name", &name), ("timeout", "60")],
                Some("checkNetwork"),
            ))
        })??;

        let result = (|| -> Result<GlobalSkill> {
            let mut remote_skills = Vec::new();
            self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills)?;
            let remote = remote_skills
                .iter()
                .find(|candidate| {
                    candidate
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&candidate.directory)
                        .eq_ignore_ascii_case(&skill.directory)
                })
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &skill.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            let source =
                Self::resolve_skill_source_dir(&temp_dir, &remote.directory).ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &remote.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            Self::replace_dest_with_copy(&source, &dest, &skill.directory)?;
            Self::save_updated_global_skill(db, &skill, &owner, &name, &used_branch, &dest)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 更新指定 CLI 原生目录中的单个 Skill，不修改其他 CLI 或全局库。
    pub async fn update_app_skill(
        &self,
        db: &Arc<Database>,
        app: &AppType,
        skill_id: &str,
    ) -> Result<AppSkill> {
        Self::ensure_app_skill_support(app)?;
        let snapshot = Self::get_for_app(db, app)?;
        let app_skill = snapshot
            .skills
            .into_iter()
            .find(|skill| skill.id == skill_id)
            .ok_or_else(|| anyhow!("Skill not found in {}: {skill_id}", app.as_str()))?;
        if app_skill.managed_globally {
            return Err(anyhow!(
                "Global linked Skills must be updated from the global library"
            ));
        }
        let (owner, name, branch) = match (&app_skill.repo_owner, &app_skill.repo_name) {
            (Some(owner), Some(name)) => (
                owner.clone(),
                name.clone(),
                app_skill
                    .repo_branch
                    .clone()
                    .unwrap_or_else(|| "main".to_string()),
            ),
            _ => return Err(anyhow!("Cannot update local Skill: {skill_id}")),
        };
        let repo = SkillRepo {
            owner: owner.clone(),
            name: name.clone(),
            branch: branch.clone(),
            enabled: true,
        };
        let dest = PathBuf::from(&app_skill.path);
        Self::validate_sync_source_dir(&dest, &app_skill.directory)?;

        let records = db.get_all_installed_skills()?;
        let backup_record = records
            .get(skill_id)
            .cloned()
            .unwrap_or_else(|| InstalledSkill {
                id: skill_id.to_string(),
                name: app_skill.name.clone(),
                description: app_skill.description.clone(),
                directory: app_skill.directory.clone(),
                repo_owner: app_skill.repo_owner.clone(),
                repo_name: app_skill.repo_name.clone(),
                repo_branch: app_skill.repo_branch.clone(),
                readme_url: app_skill.readme_url.clone(),
                apps: SkillApps::only(app),
                installed_at: app_skill.installed_at,
                content_hash: app_skill.content_hash.clone(),
                updated_at: app_skill.updated_at,
            });
        let _ = Self::create_backup_from_source(&backup_record, &dest, Some(app.clone()))?;

        let command_home = Self::skills_cli_workspace_for_app(app);
        match Self::try_update_remote_with_skills_cli(
            &command_home,
            &dest,
            &app_skill.directory,
            false,
        )
        .await
        {
            Ok(recorded_branch) => {
                let updated_branch = recorded_branch.unwrap_or(branch);
                return Self::save_updated_app_skill(
                    db,
                    app,
                    &app_skill,
                    skill_id,
                    (&owner, &name, &updated_branch),
                    &dest,
                );
            }
            Err(command_error) => {
                log::warn!(
                    "skills CLI update failed for {} Skill {}; falling back to archive: {command_error}",
                    app.as_str(),
                    app_skill.directory
                );
            }
        }

        let (temp_dir, used_branch) = timeout(
            std::time::Duration::from_secs(60),
            self.download_repo(&repo),
        )
        .await
        .map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_TIMEOUT",
                &[("owner", &owner), ("name", &name), ("timeout", "60")],
                Some("checkNetwork"),
            ))
        })??;

        let result = (|| -> Result<AppSkill> {
            let mut remote_skills = Vec::new();
            self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills)?;
            let install_name = app_skill
                .directory
                .rsplit('/')
                .next()
                .unwrap_or(&app_skill.directory);
            let remote = remote_skills
                .iter()
                .find(|candidate| {
                    candidate
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&candidate.directory)
                        .eq_ignore_ascii_case(install_name)
                })
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &app_skill.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            let source =
                Self::resolve_skill_source_dir(&temp_dir, &remote.directory).ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &remote.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            Self::replace_dest_with_copy(&source, &dest, &app_skill.directory)?;
            Self::save_updated_app_skill(
                db,
                app,
                &app_skill,
                skill_id,
                (&owner, &name, &used_branch),
                &dest,
            )
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 更新单个 Skill（重新下载并替换本地文件）
    pub async fn update_skill(&self, db: &Arc<Database>, skill_id: &str) -> Result<InstalledSkill> {
        let skill = db
            .get_installed_skill(skill_id)?
            .ok_or_else(|| anyhow!("Skill not found: {skill_id}"))?;

        let (owner, name, branch) = match (&skill.repo_owner, &skill.repo_name) {
            (Some(o), Some(n)) => (
                o.clone(),
                n.clone(),
                skill
                    .repo_branch
                    .clone()
                    .unwrap_or_else(|| "main".to_string()),
            ),
            _ => return Err(anyhow!("Cannot update local skill: {skill_id}")),
        };

        let ssot_dir = Self::get_ssot_dir()?;
        let dest = ssot_dir.join(&skill.directory);
        Self::validate_sync_source_dir(&dest, &skill.directory)?;
        let _ = Self::create_backup_from_source(&skill, &dest, None)?;

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
        match Self::try_update_remote_with_skills_cli(
            &command_home,
            &dest,
            &skill.directory,
            global_command,
        )
        .await
        {
            Ok(recorded_branch) => {
                let updated_branch = recorded_branch.unwrap_or(branch);
                return Self::save_updated_unified_skill(
                    db,
                    &skill,
                    &owner,
                    &name,
                    &updated_branch,
                    &dest,
                );
            }
            Err(command_error) => {
                log::warn!(
                    "skills CLI update failed for compatible Skill {}; falling back to archive: {command_error}",
                    skill.directory
                );
            }
        }

        let repo = SkillRepo {
            owner: owner.clone(),
            name: name.clone(),
            branch: branch.clone(),
            enabled: true,
        };

        let (temp_dir, used_branch) = timeout(
            std::time::Duration::from_secs(60),
            self.download_repo(&repo),
        )
        .await
        .map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_TIMEOUT",
                &[("owner", &owner), ("name", &name), ("timeout", "60")],
                Some("checkNetwork"),
            ))
        })??;

        let result = (|| -> Result<InstalledSkill> {
            let mut remote_skills = Vec::new();
            self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills)?;
            let remote_match = remote_skills
                .iter()
                .find(|remote| {
                    remote
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&remote.directory)
                        .eq_ignore_ascii_case(&skill.directory)
                })
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &skill.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            let source = Self::resolve_skill_source_dir(&temp_dir, &remote_match.directory)
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &remote_match.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            Self::replace_dest_with_copy(&source, &dest, &skill.directory)?;
            Self::save_updated_unified_skill(db, &skill, &owner, &name, &used_branch, &dest)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 为缺少 content_hash 的已安装 Skill 补算哈希
    pub fn backfill_content_hashes(db: &Arc<Database>) -> Result<usize> {
        let skills = db.get_all_installed_skills()?;
        let ssot_dir = Self::get_ssot_dir()?;
        let mut count = 0;

        for skill in skills.values() {
            if skill.content_hash.is_some() {
                continue;
            }
            let skill_dir = ssot_dir.join(&skill.directory);
            if !skill_dir.exists() {
                continue;
            }
            match Self::compute_dir_hash(&skill_dir) {
                Ok(hash) => {
                    let _ = db.update_skill_hash(&skill.id, &hash, 0);
                    count += 1;
                }
                Err(e) => {
                    log::warn!("补算哈希失败 {}: {e}", skill.id);
                }
            }
        }

        if count > 0 {
            log::info!("已为 {count} 个 Skill 补算内容哈希");
        }
        Ok(count)
    }
}
