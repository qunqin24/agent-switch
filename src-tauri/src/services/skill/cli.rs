use super::*;

impl SkillService {
    pub fn new() -> Self {
        Self
    }

    pub(super) fn skills_cli_workspace_for_app(app: &AppType) -> PathBuf {
        get_app_config_dir().join("skills-cli").join(app.as_str())
    }

    pub(super) fn skills_cli_workspace_for_unified() -> PathBuf {
        get_app_config_dir().join("skills-cli").join("unified")
    }

    pub(super) fn skills_cli_canonical_skill_path(workspace: &Path, install_name: &str) -> PathBuf {
        workspace.join(".agents").join("skills").join(install_name)
    }

    pub(super) fn validate_skills_cli_identifier(label: &str, value: &str) -> Result<()> {
        if value.is_empty()
            || !value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
        {
            return Err(anyhow!("Invalid skills CLI {label}: {value}"));
        }
        Ok(())
    }

    pub(super) fn skills_cli_add_args(
        skill: &DiscoverableSkill,
        install_name: &str,
        global: bool,
    ) -> Result<Vec<String>> {
        Self::validate_skills_cli_identifier("owner", &skill.repo_owner)?;
        Self::validate_skills_cli_identifier("repository", &skill.repo_name)?;
        Self::validate_skills_cli_identifier("skill", install_name)?;

        let mut args = vec![
            "add".to_string(),
            format!("{}/{}", skill.repo_owner, skill.repo_name),
            "--skill".to_string(),
            install_name.to_string(),
        ];
        if global {
            args.push("--global".to_string());
        }
        args.extend([
            "--agent".to_string(),
            SKILLS_CLI_CANONICAL_AGENT.to_string(),
            "--yes".to_string(),
        ]);
        Ok(args)
    }

    pub(super) fn skills_cli_update_args(install_name: &str, global: bool) -> Result<Vec<String>> {
        Self::validate_skills_cli_identifier("skill", install_name)?;
        let mut args = vec!["update".to_string(), install_name.to_string()];
        if global {
            args.push("--global".to_string());
        }
        args.push("--yes".to_string());
        Ok(args)
    }

    pub(super) fn resolve_skills_npx_executable() -> Result<PathBuf> {
        if let Some(override_path) = std::env::var_os("AGENTSWITCH_SKILLS_NPX") {
            let override_path = PathBuf::from(override_path);
            if override_path.is_file() {
                return Ok(override_path);
            }
        }

        for directory in crate::commands::build_tool_search_paths("npx") {
            for candidate in crate::commands::tool_executable_candidates("npx", &directory) {
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }

        Err(anyhow!("npx was not found in the user CLI search paths"))
    }

    pub(super) fn skills_cli_path(executable: &Path) -> Result<std::ffi::OsString> {
        let mut paths = Vec::new();
        if let Some(parent) = executable.parent() {
            paths.push(parent.to_path_buf());
        }
        if let Some(current) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&current));
        }
        std::env::join_paths(paths).context("Failed to prepare PATH for skills CLI")
    }

    pub(super) async fn run_skills_cli(workspace: &Path, args: &[String]) -> Result<String> {
        fs::create_dir_all(workspace).with_context(|| {
            format!(
                "Failed to create skills CLI workspace: {}",
                workspace.display()
            )
        })?;
        let executable = Self::resolve_skills_npx_executable()?;
        let command_path = Self::skills_cli_path(&executable)?;
        let user_home = dirs::home_dir();

        let mut command = Command::new(&executable);
        command
            .arg("-y")
            .arg("skills")
            .args(args)
            .current_dir(workspace)
            .env("PATH", command_path)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if std::env::var_os("NPM_CONFIG_USERCONFIG").is_none() {
            if let Some(npmrc) = user_home.as_ref().map(|home| home.join(".npmrc")) {
                if npmrc.is_file() {
                    command.env("NPM_CONFIG_USERCONFIG", npmrc);
                }
            }
        }
        if std::env::var_os("NPM_CONFIG_CACHE").is_none() {
            if let Some(cache) = user_home.as_ref().map(|home| home.join(".npm")) {
                if cache.is_dir() {
                    command.env("NPM_CONFIG_CACHE", cache);
                }
            }
        }

        let output = timeout(
            std::time::Duration::from_secs(SKILLS_CLI_TIMEOUT_SECONDS),
            command.output(),
        )
        .await
        .map_err(|_| {
            anyhow!("skills CLI timed out after {SKILLS_CLI_TIMEOUT_SECONDS} seconds")
        })??;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if !output.status.success() {
            let detail = if stderr.is_empty() { &stdout } else { &stderr };
            return Err(anyhow!(
                "skills CLI exited with {}: {}",
                output
                    .status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "signal".to_string()),
                detail
                    .lines()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        log::info!("skills CLI succeeded: npx -y skills {}", args.join(" "));
        Ok(if stdout.is_empty() { stderr } else { stdout })
    }

    pub(super) async fn try_install_remote_with_skills_cli(
        &self,
        skill: &DiscoverableSkill,
        command_home: &Path,
        dest: &Path,
        install_name: &str,
        global: bool,
    ) -> Result<String> {
        let args = Self::skills_cli_add_args(skill, install_name, global)?;
        Self::run_skills_cli(command_home, &args).await?;

        let source = Self::skills_cli_canonical_skill_path(command_home, install_name);
        Self::validate_sync_source_dir(&source, install_name)?;
        if !Self::paths_are_same(&source, dest) {
            Self::copy_skill_to_new_dest(&source, dest, install_name)?;
        }

        Ok(parse_skills_cli_lock(command_home, global)
            .get(install_name)
            .and_then(|entry| entry.branch.clone())
            .unwrap_or_else(|| skill.repo_branch.clone()))
    }

    pub(super) async fn install_remote_from_archive(
        &self,
        skill: &DiscoverableSkill,
        dest: &Path,
        install_name: &str,
    ) -> Result<String> {
        let source_rel = Self::sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            anyhow!(format_skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            ))
        })?;
        let repo = SkillRepo {
            owner: skill.repo_owner.clone(),
            name: skill.repo_name.clone(),
            branch: skill.repo_branch.clone(),
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
                &[
                    ("owner", &repo.owner),
                    ("name", &repo.name),
                    ("timeout", "60"),
                ],
                Some("checkNetwork"),
            ))
        })??;

        let result = (|| -> Result<()> {
            let source =
                Self::resolve_skill_source_dir(&temp_dir, &skill.directory).ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &temp_dir.join(&source_rel).display().to_string())],
                        Some("checkRepoUrl"),
                    ))
                })?;
            let canonical_temp = temp_dir.canonicalize().unwrap_or_else(|_| temp_dir.clone());
            let canonical_source = source.canonicalize().map_err(|_| {
                anyhow!(format_skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", &source.display().to_string())],
                    Some("checkRepoUrl"),
                ))
            })?;
            if !canonical_source.starts_with(&canonical_temp) || !canonical_source.is_dir() {
                return Err(anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                )));
            }
            Self::copy_skill_to_new_dest(&canonical_source, dest, install_name)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result?;
        Ok(used_branch)
    }

    pub(super) async fn install_remote_command_first(
        &self,
        skill: &DiscoverableSkill,
        command_home: &Path,
        dest: &Path,
        install_name: &str,
        global: bool,
    ) -> Result<String> {
        match self
            .try_install_remote_with_skills_cli(skill, command_home, dest, install_name, global)
            .await
        {
            Ok(branch) => Ok(branch),
            Err(command_error) => {
                log::warn!(
                    "skills CLI install failed for {}/{}:{}; falling back to archive: {command_error}",
                    skill.repo_owner,
                    skill.repo_name,
                    install_name,
                );
                if dest.exists() || Self::is_symlink(dest) {
                    Self::remove_path(dest)?;
                }
                self.install_remote_from_archive(skill, dest, install_name)
                    .await
                    .with_context(|| format!("skills CLI install failed first: {command_error}"))
            }
        }
    }

    pub(super) async fn try_update_remote_with_skills_cli(
        command_home: &Path,
        dest: &Path,
        install_name: &str,
        global: bool,
    ) -> Result<Option<String>> {
        let before_hash = Self::compute_dir_hash(dest).ok();
        let args = Self::skills_cli_update_args(install_name, global)?;
        Self::run_skills_cli(command_home, &args).await?;

        let source = Self::skills_cli_canonical_skill_path(command_home, install_name);
        Self::validate_sync_source_dir(&source, install_name)?;
        let source_hash = Self::compute_dir_hash(&source)?;

        if !Self::paths_are_same(&source, dest)
            && before_hash.as_deref() != Some(source_hash.as_str())
        {
            Self::replace_dest_with_copy(&source, dest, install_name)?;
        }

        Ok(parse_skills_cli_lock(command_home, global)
            .get(install_name)
            .and_then(|entry| entry.branch.clone()))
    }

    /// 构建 Skill 文档 URL（指向仓库中的 SKILL.md 文件）
    pub(super) fn build_skill_doc_url(
        owner: &str,
        repo: &str,
        branch: &str,
        doc_path: &str,
    ) -> String {
        format!("https://github.com/{owner}/{repo}/blob/{branch}/{doc_path}")
    }

    /// 从旧 readme_url 中提取仓库内文档路径，兼容 `blob`/`tree` 两种格式
    pub(super) fn extract_doc_path_from_url(url: &str) -> Option<String> {
        let marker = if url.contains("/blob/") {
            "/blob/"
        } else if url.contains("/tree/") {
            "/tree/"
        } else {
            return None;
        };

        let (_, tail) = url.split_once(marker)?;
        let (_, path) = tail.split_once('/')?;
        if path.is_empty() {
            return None;
        }
        Some(path.to_string())
    }
}
