use super::*;

// ========== 从 ZIP 文件安装 ==========

impl SkillService {
    /// 从 ZIP 直接安装到指定 CLI 的原生 Skills 目录。
    pub fn install_from_zip_for_app(
        db: &Arc<Database>,
        zip_path: &Path,
        app: &AppType,
    ) -> Result<Vec<AppSkill>> {
        Self::ensure_app_skill_support(app)?;
        if Self::app_primary_skills_dir_is_global(app) {
            let installed = Self::install_from_zip_global(db, zip_path)?;
            let global_dir = Self::get_global_skills_dir()?;
            let records = db.get_all_installed_skills()?;
            return Ok(installed
                .into_iter()
                .map(|skill| {
                    let metadata = Self::find_global_record(records.values(), &skill.directory);
                    Self::app_skill_from_path(
                        app,
                        &global_dir.join(&skill.directory),
                        &skill.directory,
                        metadata,
                    )
                })
                .collect());
        }
        let temp_dir = Self::extract_local_zip(zip_path)?;
        let result = (|| -> Result<Vec<AppSkill>> {
            let skill_dirs = Self::scan_skills_in_dir(&temp_dir)?;
            if skill_dirs.is_empty() {
                return Err(anyhow!(format_skill_error(
                    "NO_SKILLS_IN_ZIP",
                    &[],
                    Some("checkZipContent"),
                )));
            }

            let app_dir = Self::get_app_skills_dir(app)?;
            fs::create_dir_all(&app_dir)?;
            let zip_stem = zip_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string);
            let mut installed = Vec::new();

            for skill_dir in skill_dirs {
                let skill_md = skill_dir.join("SKILL.md");
                let metadata = Self::parse_skill_metadata_static(&skill_md).ok();
                let raw_directory = skill_dir
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default();
                let install_name = if skill_dir == temp_dir || raw_directory.starts_with('.') {
                    metadata
                        .as_ref()
                        .and_then(|value| value.name.as_deref())
                        .and_then(Self::sanitize_install_name)
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                } else {
                    Self::sanitize_install_name(&raw_directory)
                        .or_else(|| {
                            metadata
                                .as_ref()
                                .and_then(|value| value.name.as_deref())
                                .and_then(Self::sanitize_install_name)
                        })
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                }
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "INVALID_SKILL_DIRECTORY",
                        &[("zip", &zip_path.display().to_string())],
                        Some("checkZipContent"),
                    ))
                })?;

                let dest = app_dir.join(&install_name);
                if dest.exists() || Self::is_symlink(&dest) {
                    log::warn!(
                        "Skill directory '{}' already exists in {}, skipping",
                        install_name,
                        app.as_str()
                    );
                    continue;
                }

                Self::copy_skill_to_new_dest(&skill_dir, &dest, &install_name)?;
                let (name, description) =
                    Self::read_skill_name_desc(&dest.join("SKILL.md"), &install_name);
                let record = InstalledSkill {
                    id: format!("{}:local:{install_name}", app.as_str()),
                    name,
                    description,
                    directory: install_name.clone(),
                    repo_owner: None,
                    repo_name: None,
                    repo_branch: None,
                    readme_url: None,
                    apps: SkillApps::only(app),
                    installed_at: Utc::now().timestamp(),
                    content_hash: Self::compute_dir_hash(&dest).ok(),
                    updated_at: 0,
                };
                if let Err(error) = db.save_skill(&record) {
                    let _ = Self::remove_path(&dest);
                    return Err(error.into());
                }
                installed.push(Self::app_skill_from_path(
                    app,
                    &dest,
                    &install_name,
                    Some(&record),
                ));
            }

            Ok(installed)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 将 ZIP 中的 Skills 安装到全局目录；不会自动创建额外的 CLI 链接。
    pub fn install_from_zip_global(
        db: &Arc<Database>,
        zip_path: &Path,
    ) -> Result<Vec<GlobalSkill>> {
        let temp_dir = Self::extract_local_zip(zip_path)?;
        let result = (|| -> Result<Vec<GlobalSkill>> {
            let skill_dirs = Self::scan_skills_in_dir(&temp_dir)?;
            if skill_dirs.is_empty() {
                return Err(anyhow!(format_skill_error(
                    "NO_SKILLS_IN_ZIP",
                    &[],
                    Some("checkZipContent"),
                )));
            }

            let global_dir = Self::get_global_skills_dir()?;
            let zip_stem = zip_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string);
            let mut installed = Vec::new();

            for skill_dir in skill_dirs {
                let skill_md = skill_dir.join("SKILL.md");
                let metadata = Self::parse_skill_metadata_static(&skill_md).ok();
                let raw_directory = skill_dir
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default();
                let install_name = if skill_dir == temp_dir || raw_directory.starts_with('.') {
                    metadata
                        .as_ref()
                        .and_then(|value| value.name.as_deref())
                        .and_then(Self::sanitize_install_name)
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                } else {
                    Self::sanitize_install_name(&raw_directory)
                        .or_else(|| {
                            metadata
                                .as_ref()
                                .and_then(|value| value.name.as_deref())
                                .and_then(Self::sanitize_install_name)
                        })
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                }
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "INVALID_SKILL_DIRECTORY",
                        &[("zip", &zip_path.display().to_string())],
                        Some("checkZipContent"),
                    ))
                })?;

                let dest = global_dir.join(&install_name);
                if dest.exists() || Self::is_symlink(&dest) {
                    log::warn!(
                        "Skill directory '{}' already exists in global library, skipping",
                        install_name
                    );
                    continue;
                }

                Self::copy_skill_to_new_dest(&skill_dir, &dest, &install_name)?;
                let (name, description) =
                    Self::read_skill_name_desc(&dest.join("SKILL.md"), &install_name);
                let record = InstalledSkill {
                    id: format!("global:local:{install_name}"),
                    name,
                    description,
                    directory: install_name.clone(),
                    repo_owner: None,
                    repo_name: None,
                    repo_branch: None,
                    readme_url: None,
                    apps: Self::global_link_states(&dest, &install_name),
                    installed_at: Utc::now().timestamp(),
                    content_hash: Self::compute_dir_hash(&dest).ok(),
                    updated_at: 0,
                };
                if let Err(error) = db.save_skill(&record) {
                    let _ = Self::remove_path(&dest);
                    return Err(error.into());
                }
                installed.push(Self::global_skill_from_path(
                    &dest,
                    &install_name,
                    Some(&record),
                ));
            }

            Ok(installed)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 从本地 ZIP 文件安装 Skills
    ///
    /// 流程：
    /// 1. 解压 ZIP 到临时目录
    /// 2. 扫描目录查找包含 SKILL.md 的技能
    /// 3. 复制到 SSOT 并保存到数据库
    /// 4. 同步到当前应用目录
    pub fn install_from_zip(
        db: &Arc<Database>,
        zip_path: &Path,
        current_app: &AppType,
    ) -> Result<Vec<InstalledSkill>> {
        // 解压到临时目录
        let temp_dir = Self::extract_local_zip(zip_path)?;

        // 扫描所有包含 SKILL.md 的目录
        let skill_dirs = Self::scan_skills_in_dir(&temp_dir)?;

        if skill_dirs.is_empty() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(anyhow!(format_skill_error(
                "NO_SKILLS_IN_ZIP",
                &[],
                Some("checkZipContent"),
            )));
        }

        let ssot_dir = Self::get_ssot_dir()?;
        let mut installed = Vec::new();
        let existing_skills = db.get_all_installed_skills()?;
        let zip_stem = zip_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        for skill_dir in skill_dirs {
            // 解析元数据（提前解析，用于确定安装名）
            let skill_md = skill_dir.join("SKILL.md");
            let meta = if skill_md.exists() {
                Self::parse_skill_metadata_static(&skill_md).ok()
            } else {
                None
            };

            // 获取目录名称作为安装名
            // 当 SKILL.md 在 ZIP 根目录时，skill_dir == temp_dir，
            // file_name() 会返回临时目录名（如 .tmpDZKGpF），需要回退到其他来源
            let install_name = {
                let dir_name = skill_dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                if skill_dir == temp_dir || dir_name.is_empty() || dir_name.starts_with('.') {
                    // SKILL.md 在根目录：优先用元数据 name，否则用 ZIP 文件名
                    meta.as_ref()
                        .and_then(|m| m.name.as_deref())
                        .and_then(Self::sanitize_install_name)
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                } else {
                    Self::sanitize_install_name(&dir_name)
                        .or_else(|| {
                            meta.as_ref()
                                .and_then(|m| m.name.as_deref())
                                .and_then(Self::sanitize_install_name)
                        })
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                }
            };
            let install_name = match install_name {
                Some(name) => name,
                None => {
                    let _ = fs::remove_dir_all(&temp_dir);
                    return Err(anyhow!(format_skill_error(
                        "INVALID_SKILL_DIRECTORY",
                        &[("zip", &zip_path.display().to_string())],
                        Some("checkZipContent"),
                    )));
                }
            };

            // 检查是否已有同名 directory 的 skill
            let conflict = existing_skills
                .values()
                .find(|s| s.directory.eq_ignore_ascii_case(&install_name));

            if let Some(existing) = conflict {
                log::warn!(
                    "Skill directory '{}' already exists (from {}), skipping",
                    install_name,
                    existing.id
                );
                continue;
            }

            let (name, description) = match meta {
                Some(m) => (
                    m.name.unwrap_or_else(|| install_name.clone()),
                    m.description,
                ),
                None => (install_name.clone(), None),
            };

            // 复制到 SSOT
            let dest = ssot_dir.join(&install_name);
            if dest.exists() {
                let _ = fs::remove_dir_all(&dest);
            }
            Self::copy_dir_recursive(&skill_dir, &dest)?;

            // 计算内容哈希
            let content_hash = Self::compute_dir_hash(&dest).ok();

            // 创建 InstalledSkill 记录
            let skill = InstalledSkill {
                id: format!("local:{install_name}"),
                name,
                description,
                directory: install_name.clone(),
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                apps: SkillApps::only(current_app),
                installed_at: chrono::Utc::now().timestamp(),
                content_hash,
                updated_at: 0,
            };

            // 保存到数据库
            db.save_skill(&skill)?;

            // 同步到当前应用目录
            Self::sync_to_app_dir(&install_name, current_app)?;

            log::info!(
                "Skill {} installed from ZIP, enabled for {:?}",
                skill.name,
                current_app
            );
            installed.push(skill);
        }

        // 清理临时目录
        let _ = fs::remove_dir_all(&temp_dir);

        Ok(installed)
    }

    /// 解压本地 ZIP 文件到临时目录
    pub(super) fn extract_local_zip(zip_path: &Path) -> Result<PathBuf> {
        let file = fs::File::open(zip_path)
            .with_context(|| format!("Failed to open ZIP file: {}", zip_path.display()))?;

        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("Failed to read ZIP file: {}", zip_path.display()))?;

        if archive.is_empty() {
            return Err(anyhow!(format_skill_error(
                "EMPTY_ARCHIVE",
                &[],
                Some("checkZipContent"),
            )));
        }

        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().to_path_buf();
        let _ = temp_dir.keep(); // Keep the directory, we'll clean up later

        let mut symlinks: Vec<(PathBuf, String)> = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let file_path = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => continue,
            };

            let outpath = temp_path.join(&file_path);

            if file.is_symlink() {
                let mut target = String::new();
                std::io::Read::read_to_string(&mut file, &mut target)?;
                symlinks.push((outpath, target.trim().to_string()));
            } else if file.is_dir() {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        // 解析 symlink
        Self::resolve_symlinks_in_dir(&temp_path, &symlinks)?;

        Ok(temp_path)
    }

    /// 递归扫描目录查找包含 SKILL.md 的技能目录
    pub(super) fn scan_skills_in_dir(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut skill_dirs = Vec::new();
        Self::scan_skills_recursive(dir, &mut skill_dirs)?;
        Ok(skill_dirs)
    }

    /// 递归扫描辅助函数
    pub(super) fn scan_skills_recursive(current: &Path, results: &mut Vec<PathBuf>) -> Result<()> {
        // 检查当前目录是否包含 SKILL.md
        let skill_md = current.join("SKILL.md");
        if skill_md.exists() {
            results.push(current.to_path_buf());
            // 找到后不再递归子目录（一个 skill 目录）
            return Ok(());
        }

        // 递归子目录
        if let Ok(entries) = fs::read_dir(current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // 跳过隐藏目录
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    if dir_name.starts_with('.') {
                        continue;
                    }
                    Self::scan_skills_recursive(&path, results)?;
                }
            }
        }

        Ok(())
    }
}
