use std::fs;

use agent_switch_lib::{
    migrate_skills_to_ssot, AppType, ImportSkillSelection, InstalledSkill, SkillApps, SkillService,
};

#[path = "support.rs"]
mod support;
use support::{create_test_state, ensure_test_home, reset_test_fs, test_mutex};

fn write_skill(dir: &std::path::Path, name: &str) {
    fs::create_dir_all(dir).expect("create skill dir");
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Test skill\n---\n"),
    )
    .expect("write SKILL.md");
}

#[test]
fn app_skill_listing_uses_only_the_selected_cli_directory() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    write_skill(
        &home.join(".agents").join("skills").join("codex-only"),
        "Codex Only",
    );
    write_skill(
        &home.join(".claude").join("skills").join("claude-only"),
        "Claude Only",
    );
    let state = create_test_state().expect("create test state");

    let result = SkillService::get_for_app(&state.db, &AppType::Codex).expect("list Codex skills");

    assert_eq!(result.app, "codex");
    assert_eq!(
        result.skills_dir,
        home.join(".agents")
            .join("skills")
            .to_string_lossy()
            .to_string()
    );
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].directory, "codex-only");
    assert_eq!(result.skills[0].name, "Codex Only");
}

#[test]
fn hermes_skill_listing_supports_category_directories() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    write_skill(
        &home
            .join(".hermes")
            .join("skills")
            .join("research")
            .join("paper-review"),
        "Paper Review",
    );
    let state = create_test_state().expect("create test state");

    let result =
        SkillService::get_for_app(&state.db, &AppType::Hermes).expect("list Hermes skills");

    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].directory, "research/paper-review");
    assert_eq!(result.skills[0].name, "Paper Review");
}

#[test]
fn app_skill_uninstall_only_changes_the_selected_non_global_cli() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let gemini_skill = home.join(".gemini").join("skills").join("shared-skill");
    let claude_skill = home.join(".claude").join("skills").join("shared-skill");
    let ssot_skill = home
        .join(".agentswitch")
        .join("skills")
        .join("shared-skill");
    write_skill(&gemini_skill, "Gemini Version");
    write_skill(&claude_skill, "Claude Version");
    write_skill(&ssot_skill, "Legacy SSOT Version");
    fs::write(gemini_skill.join("prompt.md"), "gemini content").expect("write Gemini content");
    fs::write(claude_skill.join("prompt.md"), "claude content").expect("write Claude content");

    let state = create_test_state().expect("create test state");
    state
        .db
        .save_skill(&InstalledSkill {
            id: "local:shared-skill".to_string(),
            name: "Shared".to_string(),
            description: None,
            directory: "shared-skill".to_string(),
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            apps: SkillApps {
                claude: true,
                gemini: true,
                ..Default::default()
            },
            installed_at: 1,
            content_hash: None,
            updated_at: 0,
        })
        .expect("save legacy metadata");

    let result = SkillService::uninstall_for_app(&state.db, &AppType::Gemini, "shared-skill")
        .expect("uninstall Gemini skill");

    assert!(!gemini_skill.exists(), "Gemini directory should be removed");
    assert!(
        claude_skill.exists(),
        "the same skill in Claude must remain untouched"
    );
    assert!(
        ssot_skill.exists(),
        "legacy SSOT content must not be removed by a per-CLI uninstall"
    );
    assert_eq!(
        fs::read_to_string(claude_skill.join("prompt.md")).expect("read Claude content"),
        "claude content"
    );

    let record = state
        .db
        .get_installed_skill("local:shared-skill")
        .expect("query metadata")
        .expect("metadata remains for Claude");
    assert!(record.apps.claude);
    assert!(!record.apps.gemini);

    let backup_path = std::path::PathBuf::from(result.backup_path.expect("backup path"));
    assert_eq!(
        fs::read_to_string(backup_path.join("skill").join("prompt.md"))
            .expect("read backup content"),
        "gemini content",
        "the backup must come from the selected CLI, not the legacy SSOT"
    );
    let metadata: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(backup_path.join("meta.json")).expect("read backup metadata"),
    )
    .expect("parse backup metadata");
    assert_eq!(metadata["sourceApp"], "gemini");

    let backup_id = backup_path
        .file_name()
        .and_then(|value| value.to_str())
        .expect("backup id");
    assert!(
        SkillService::list_backups_for_app(&AppType::Gemini)
            .expect("list Gemini backups")
            .iter()
            .any(|entry| entry.backup_id == backup_id),
        "the backup must stay scoped to Gemini"
    );
    assert!(
        SkillService::list_global_backups()
            .expect("list global backups")
            .iter()
            .all(|entry| entry.backup_id != backup_id),
        "a CLI backup must not appear in global management"
    );
    let restored = SkillService::restore_for_app(&state.db, backup_id, &AppType::Gemini)
        .expect("restore Gemini backup");
    assert_eq!(restored.directory, "shared-skill");
    assert_eq!(
        fs::read_to_string(gemini_skill.join("prompt.md")).expect("read restored Gemini content"),
        "gemini content"
    );
    assert_eq!(
        fs::read_to_string(claude_skill.join("prompt.md")).expect("read preserved Claude content"),
        "claude content"
    );
}

#[test]
fn global_skill_availability_combines_native_readers_and_explicit_links() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let global_skill = home.join(".agents").join("skills").join("global-skill");
    write_skill(&global_skill, "Global Skill");
    fs::write(global_skill.join("prompt.md"), "global content").expect("write global content");
    let state = create_test_state().expect("create test state");

    let initial = SkillService::get_global(&state.db).expect("list global skills");
    assert_eq!(
        initial.skills_dir,
        home.join(".agents")
            .join("skills")
            .to_string_lossy()
            .to_string()
    );
    assert_eq!(initial.skills.len(), 1);
    assert!(!initial.direct_apps.claude);
    assert!(initial.direct_apps.codex);
    assert!(initial.direct_apps.gemini);
    assert!(initial.direct_apps.opencode);
    assert!(!initial.direct_apps.hermes);
    assert!(initial.skills[0].apps.codex);
    assert!(initial.skills[0].apps.gemini);
    assert!(initial.skills[0].apps.opencode);

    let linked = SkillService::set_global_link(&state.db, "global-skill", &AppType::Claude, true)
        .expect("link global skill to Claude");
    assert!(linked.apps.codex);
    assert!(linked.apps.claude);

    let claude_path = home.join(".claude").join("skills").join("global-skill");
    assert!(
        fs::symlink_metadata(&claude_path)
            .expect("read Claude link metadata")
            .file_type()
            .is_symlink(),
        "global enablement must create a symbolic link"
    );
    assert!(
        !home
            .join(".gemini")
            .join("skills")
            .join("global-skill")
            .exists(),
        "linking Claude must not modify Gemini"
    );

    let claude =
        SkillService::get_for_app(&state.db, &AppType::Claude).expect("list linked Claude skill");
    assert_eq!(claude.skills.len(), 1);
    assert!(claude.skills[0].is_symlink);
    assert!(claude.skills[0].managed_globally);
    assert!(!claude.skills[0].global_source);

    let codex =
        SkillService::get_for_app(&state.db, &AppType::Codex).expect("list native global skill");
    assert_eq!(codex.skills.len(), 1);
    assert!(codex.skills[0].managed_globally);
    assert!(codex.skills[0].global_source);

    SkillService::set_global_link(&state.db, "global-skill", &AppType::Claude, false)
        .expect("unlink global skill from Claude");
    assert!(
        !claude_path.exists() && fs::symlink_metadata(&claude_path).is_err(),
        "disabling the global Skill must remove only the link"
    );
    assert!(
        global_skill.exists(),
        "disabling a CLI link must preserve the global source"
    );
    let error = SkillService::set_global_link(&state.db, "global-skill", &AppType::Codex, false)
        .expect_err("a direct global reader cannot be unlinked");
    assert!(error
        .to_string()
        .contains("reads the global Skills directory directly"));
    let error = SkillService::set_global_link(&state.db, "global-skill", &AppType::OpenCode, false)
        .expect_err("OpenCode also reads the global directory directly");
    assert!(error
        .to_string()
        .contains("reads the global Skills directory directly"));
    let error = SkillService::uninstall_for_app(&state.db, &AppType::Codex, "global-skill")
        .expect_err("a direct global source cannot be removed from CLI management");
    assert!(error.to_string().contains("global management"));
    assert!(global_skill.exists());
}

#[test]
fn hermes_global_skills_follow_external_dirs_configuration() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    write_skill(
        &home.join(".agents").join("skills").join("global-skill"),
        "Global Skill",
    );
    fs::create_dir_all(home.join(".hermes")).expect("create Hermes config dir");
    fs::write(
        home.join(".hermes").join("config.yaml"),
        "skills:\n  external_dirs:\n    - ~/.agents/skills\n",
    )
    .expect("write Hermes config");
    let state = create_test_state().expect("create test state");

    let global = SkillService::get_global(&state.db).expect("list global skills");

    assert!(global.direct_apps.hermes);
    assert!(global.skills[0].apps.hermes);
    let error = SkillService::set_global_link(&state.db, "global-skill", &AppType::Hermes, false)
        .expect_err("Hermes external directories do not need a managed link");
    assert!(error
        .to_string()
        .contains("reads the global Skills directory directly"));
}

#[test]
fn globally_linked_skill_cannot_be_updated_from_a_cli_scope() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    write_skill(
        &home.join(".agents").join("skills").join("global-skill"),
        "Global Skill",
    );
    let state = create_test_state().expect("create test state");
    let linked = SkillService::set_global_link(&state.db, "global-skill", &AppType::Claude, true)
        .expect("link global skill");

    let error = futures::executor::block_on(SkillService::new().update_app_skill(
        &state.db,
        &AppType::Claude,
        &linked.id,
    ))
    .expect_err("CLI update must reject a globally linked Skill");

    assert!(
        error.to_string().contains("global library"),
        "the error should direct updates to global management"
    );
}

#[test]
fn global_skill_link_never_overwrites_a_same_name_local_skill() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let global_skill = home.join(".agents").join("skills").join("shared-skill");
    let claude_skill = home.join(".claude").join("skills").join("shared-skill");
    write_skill(&global_skill, "Global Version");
    write_skill(&claude_skill, "Claude Local Version");
    fs::write(claude_skill.join("prompt.md"), "keep local").expect("write local content");
    let state = create_test_state().expect("create test state");

    let error = SkillService::set_global_link(&state.db, "shared-skill", &AppType::Claude, true)
        .expect_err("same-name local Skill must block the global link");
    assert!(error.to_string().contains("already exists"));
    assert_eq!(
        fs::read_to_string(claude_skill.join("prompt.md")).expect("read local content"),
        "keep local"
    );
    assert!(
        !fs::symlink_metadata(&claude_skill)
            .expect("read local metadata")
            .file_type()
            .is_symlink(),
        "the existing local directory must not be replaced"
    );
}

#[test]
fn uninstalling_a_global_skill_removes_only_its_links_and_can_restore_it() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let global_skill = home.join(".agents").join("skills").join("shared-skill");
    let claude_local = home.join(".claude").join("skills").join("shared-skill");
    write_skill(&global_skill, "Global Version");
    fs::write(global_skill.join("prompt.md"), "global content").expect("write global content");
    write_skill(&claude_local, "Claude Local");
    fs::write(claude_local.join("prompt.md"), "claude content").expect("write Claude content");
    let state = create_test_state().expect("create test state");

    SkillService::set_global_link(&state.db, "shared-skill", &AppType::OpenCode, true)
        .expect("confirm OpenCode direct access");

    let uninstall =
        SkillService::uninstall_global(&state.db, "shared-skill").expect("uninstall global skill");
    assert!(!global_skill.exists());
    assert!(!home
        .join(".config")
        .join("opencode")
        .join("skills")
        .join("shared-skill")
        .exists());
    assert_eq!(
        fs::read_to_string(claude_local.join("prompt.md")).expect("read Claude content"),
        "claude content",
        "an unrelated same-name local Skill must remain untouched"
    );

    let backup_path = std::path::PathBuf::from(uninstall.backup_path.expect("global backup path"));
    let backup_id = backup_path
        .file_name()
        .and_then(|value| value.to_str())
        .expect("backup id");
    assert!(
        SkillService::list_global_backups()
            .expect("list global backups")
            .iter()
            .any(|entry| entry.backup_id == backup_id),
        "the backup must stay scoped to global management"
    );
    assert!(
        SkillService::list_backups_for_app(&AppType::Codex)
            .expect("list Codex backups")
            .iter()
            .all(|entry| entry.backup_id != backup_id),
        "a global backup must not appear in the Codex backup list"
    );
    let restored =
        SkillService::restore_global(&state.db, backup_id).expect("restore global backup");
    assert_eq!(restored.directory, "shared-skill");
    assert!(restored.apps.codex);
    assert!(restored.apps.gemini);
    assert!(restored.apps.opencode);
    assert_eq!(
        fs::read_to_string(global_skill.join("prompt.md")).expect("read restored global content"),
        "global content"
    );
}

#[test]
fn global_management_accepts_symlinked_skills_without_deleting_the_link_target() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let source = home
        .join(".agentswitch")
        .join("test-sources")
        .join("linked-skill");
    let global_dir = home.join(".agents").join("skills");
    let global_link = global_dir.join("linked-skill");
    write_skill(&source, "Linked Global Skill");
    fs::write(source.join("prompt.md"), "preserve source").expect("write source content");
    fs::create_dir_all(&global_dir).expect("create global Skills directory");
    symlink_dir(&source, &global_link);
    let state = create_test_state().expect("create test state");

    let global = SkillService::get_global(&state.db).expect("list symlinked global Skill");
    assert_eq!(global.skills.len(), 1);
    assert!(global.skills[0].apps.codex);

    let codex =
        SkillService::get_for_app(&state.db, &AppType::Codex).expect("list Codex global source");
    assert_eq!(codex.skills.len(), 1);
    assert!(codex.skills[0].is_symlink);
    assert!(codex.skills[0].managed_globally);
    assert!(codex.skills[0].global_source);

    SkillService::uninstall_global(&state.db, "linked-skill")
        .expect("uninstall symlinked global Skill");
    assert!(fs::symlink_metadata(&global_link).is_err());
    assert_eq!(
        fs::read_to_string(source.join("prompt.md")).expect("read preserved source"),
        "preserve source",
        "uninstalling a global symlink must not delete its target"
    );
}

#[cfg(unix)]
fn symlink_dir(src: &std::path::Path, dest: &std::path::Path) {
    std::os::unix::fs::symlink(src, dest).expect("create symlink");
}

#[cfg(windows)]
fn symlink_dir(src: &std::path::Path, dest: &std::path::Path) {
    std::os::windows::fs::symlink_dir(src, dest).expect("create symlink");
}

#[test]
fn import_from_apps_respects_explicit_app_selection() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    write_skill(
        &home.join(".claude").join("skills").join("shared-skill"),
        "Shared",
    );
    write_skill(
        &home
            .join(".config")
            .join("opencode")
            .join("skills")
            .join("shared-skill"),
        "Shared",
    );

    let state = create_test_state().expect("create test state");

    let imported = SkillService::import_from_apps(
        &state.db,
        vec![ImportSkillSelection {
            directory: "shared-skill".to_string(),
            apps: SkillApps {
                opencode: true,
                ..Default::default()
            },
        }],
    )
    .expect("import skills");

    assert_eq!(imported.len(), 1, "expected exactly one imported skill");
    let skill = imported.first().expect("imported skill");
    assert!(
        skill.apps.opencode,
        "explicitly selected OpenCode app should remain enabled"
    );
    assert!(
        !skill.apps.claude && !skill.apps.codex && !skill.apps.gemini,
        "import should no longer infer apps from every matching source path"
    );
}

#[test]
fn import_from_apps_does_not_rewrite_selected_app_directory() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let ssot_skill_dir = home.join(".agentswitch").join("skills").join("codex-skill");
    write_skill(&ssot_skill_dir, "Stale SSOT Skill");
    fs::write(ssot_skill_dir.join("prompt.md"), "stale ssot").expect("write stale ssot prompt");

    let codex_skill_dir = home.join(".agents").join("skills").join("codex-skill");
    write_skill(&codex_skill_dir, "Live Codex Skill");
    fs::write(codex_skill_dir.join("prompt.md"), "live codex").expect("write live codex prompt");

    let state = create_test_state().expect("create test state");

    let imported = SkillService::import_from_apps(
        &state.db,
        vec![ImportSkillSelection {
            directory: "codex-skill".to_string(),
            apps: SkillApps {
                codex: true,
                ..Default::default()
            },
        }],
    )
    .expect("import skills");

    assert_eq!(imported.len(), 1, "expected exactly one imported skill");
    assert!(
        imported[0].apps.codex,
        "import should preserve the selected Codex app state"
    );
    assert_eq!(
        fs::read_to_string(codex_skill_dir.join("prompt.md")).expect("read live codex prompt"),
        "live codex",
        "import should not replace the app skill directory with SSOT contents"
    );
    assert!(
        !fs::symlink_metadata(&codex_skill_dir)
            .expect("read codex skill metadata")
            .file_type()
            .is_symlink(),
        "import should not replace the app skill directory with a managed symlink"
    );
}

#[test]
fn sync_to_app_removes_disabled_and_orphaned_ssot_symlinks() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let ssot_dir = home.join(".agentswitch").join("skills");
    let disabled_skill = ssot_dir.join("disabled-skill");
    let orphan_skill = ssot_dir.join("orphan-skill");
    write_skill(&disabled_skill, "Disabled");
    write_skill(&orphan_skill, "Orphan");

    let opencode_skills_dir = home.join(".config").join("opencode").join("skills");
    fs::create_dir_all(&opencode_skills_dir).expect("create opencode skills dir");
    symlink_dir(&disabled_skill, &opencode_skills_dir.join("disabled-skill"));
    symlink_dir(&orphan_skill, &opencode_skills_dir.join("orphan-skill"));

    let state = create_test_state().expect("create test state");
    state
        .db
        .save_skill(&InstalledSkill {
            id: "local:disabled-skill".to_string(),
            name: "Disabled".to_string(),
            description: None,
            directory: "disabled-skill".to_string(),
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            apps: SkillApps::default(),
            installed_at: 0,
            content_hash: None,
            updated_at: 0,
        })
        .expect("save disabled skill");

    SkillService::sync_to_app(&state.db, &AppType::OpenCode).expect("reconcile skills");

    assert!(
        !opencode_skills_dir.join("disabled-skill").exists(),
        "DB-known disabled skill should be removed from OpenCode live dir"
    );
    assert!(
        !opencode_skills_dir.join("orphan-skill").exists(),
        "orphaned symlink into SSOT should be cleaned up"
    );
}

#[test]
fn uninstall_skill_creates_backup_before_removing_ssot() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let ssot_skill_dir = home
        .join(".agentswitch")
        .join("skills")
        .join("backup-skill");
    write_skill(&ssot_skill_dir, "Backup Skill");
    fs::write(ssot_skill_dir.join("prompt.md"), "backup me").expect("write prompt.md");

    let state = create_test_state().expect("create test state");
    state
        .db
        .save_skill(&InstalledSkill {
            id: "local:backup-skill".to_string(),
            name: "Backup Skill".to_string(),
            description: Some("Back me up before uninstall".to_string()),
            directory: "backup-skill".to_string(),
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            apps: SkillApps {
                claude: true,
                ..Default::default()
            },
            installed_at: 123,
            content_hash: None,
            updated_at: 0,
        })
        .expect("save skill");

    let result = SkillService::uninstall(&state.db, "local:backup-skill").expect("uninstall skill");
    let backup_path = result.backup_path.expect("backup path should be returned");
    let backup_dir = std::path::PathBuf::from(&backup_path);

    assert!(backup_dir.exists(), "backup directory should exist");
    assert!(
        backup_dir.join("skill").join("SKILL.md").exists(),
        "backup should include SKILL.md"
    );
    assert_eq!(
        fs::read_to_string(backup_dir.join("skill").join("prompt.md"))
            .expect("read backed up prompt"),
        "backup me"
    );

    let metadata: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(backup_dir.join("meta.json")).expect("read backup metadata"),
    )
    .expect("parse backup metadata");
    assert_eq!(metadata["skill"]["directory"], "backup-skill");
    assert_eq!(metadata["skill"]["name"], "Backup Skill");

    assert!(
        !ssot_skill_dir.exists(),
        "SSOT skill directory should be removed after uninstall"
    );
    assert!(
        state
            .db
            .get_installed_skill("local:backup-skill")
            .expect("query skill")
            .is_none(),
        "database row should be deleted after uninstall"
    );
}

#[test]
fn restore_skill_backup_restores_files_to_ssot_and_current_app() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let ssot_skill_dir = home
        .join(".agentswitch")
        .join("skills")
        .join("restore-skill");
    write_skill(&ssot_skill_dir, "Restore Skill");
    fs::write(ssot_skill_dir.join("prompt.md"), "restore me").expect("write prompt.md");

    let state = create_test_state().expect("create test state");
    state
        .db
        .save_skill(&InstalledSkill {
            id: "local:restore-skill".to_string(),
            name: "Restore Skill".to_string(),
            description: Some("Bring the files back".to_string()),
            directory: "restore-skill".to_string(),
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            apps: SkillApps {
                claude: true,
                ..Default::default()
            },
            installed_at: 456,
            content_hash: None,
            updated_at: 0,
        })
        .expect("save skill");

    let uninstall =
        SkillService::uninstall(&state.db, "local:restore-skill").expect("uninstall skill");
    let backup_id = std::path::Path::new(
        &uninstall
            .backup_path
            .expect("backup path should be returned on uninstall"),
    )
    .file_name()
    .expect("backup dir name")
    .to_string_lossy()
    .to_string();

    let restored = SkillService::restore_from_backup(&state.db, &backup_id, &AppType::Claude)
        .expect("restore from backup");

    assert_eq!(restored.directory, "restore-skill");
    assert!(restored.apps.claude, "restored skill should enable Claude");
    assert!(
        !restored.apps.codex && !restored.apps.gemini && !restored.apps.opencode,
        "restore should only enable the selected app"
    );
    assert!(
        home.join(".agentswitch")
            .join("skills")
            .join("restore-skill")
            .join("prompt.md")
            .exists(),
        "restored skill should exist in SSOT"
    );
    assert!(
        home.join(".claude")
            .join("skills")
            .join("restore-skill")
            .join("prompt.md")
            .exists(),
        "restored skill should sync to the selected app"
    );
    assert!(
        state
            .db
            .get_installed_skill("local:restore-skill")
            .expect("query restored skill")
            .is_some(),
        "restored skill should be written back to the database"
    );
}

#[test]
fn delete_skill_backup_removes_backup_directory() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let ssot_skill_dir = home
        .join(".agentswitch")
        .join("skills")
        .join("delete-backup-skill");
    write_skill(&ssot_skill_dir, "Delete Backup Skill");

    let state = create_test_state().expect("create test state");
    state
        .db
        .save_skill(&InstalledSkill {
            id: "local:delete-backup-skill".to_string(),
            name: "Delete Backup Skill".to_string(),
            description: Some("Remove my backup".to_string()),
            directory: "delete-backup-skill".to_string(),
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            apps: SkillApps {
                claude: true,
                ..Default::default()
            },
            installed_at: 789,
            content_hash: None,
            updated_at: 0,
        })
        .expect("save skill");

    let uninstall =
        SkillService::uninstall(&state.db, "local:delete-backup-skill").expect("uninstall skill");
    let backup_path = uninstall
        .backup_path
        .expect("backup path should be returned on uninstall");
    let backup_id = std::path::Path::new(&backup_path)
        .file_name()
        .expect("backup dir name")
        .to_string_lossy()
        .to_string();

    assert!(
        std::path::Path::new(&backup_path).exists(),
        "backup directory should exist before deletion"
    );

    SkillService::delete_backup(&backup_id).expect("delete backup");

    assert!(
        !std::path::Path::new(&backup_path).exists(),
        "backup directory should be removed"
    );
    assert!(
        SkillService::list_backups()
            .expect("list backups")
            .into_iter()
            .all(|entry| entry.backup_id != backup_id),
        "deleted backup should no longer appear in backup list"
    );
}

#[test]
fn migration_snapshot_overrides_multi_source_directory_inference() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    write_skill(
        &home.join(".claude").join("skills").join("demo-skill"),
        "Demo",
    );
    write_skill(
        &home
            .join(".config")
            .join("opencode")
            .join("skills")
            .join("demo-skill"),
        "Demo",
    );

    let state = create_test_state().expect("create test state");
    state
        .db
        .set_setting(
            "skills_ssot_migration_snapshot",
            r#"[{"directory":"demo-skill","app_type":"claude"}]"#,
        )
        .expect("seed migration snapshot");

    let count = migrate_skills_to_ssot(&state.db).expect("migrate skills to ssot");
    assert_eq!(count, 1, "expected one migrated skill");

    let skills = state.db.get_all_installed_skills().expect("get skills");
    let migrated = skills
        .values()
        .find(|skill| skill.directory == "demo-skill")
        .expect("migrated demo-skill");

    assert!(
        migrated.apps.claude,
        "legacy snapshot should preserve Claude enablement"
    );
    assert!(
        !migrated.apps.opencode,
        "migration should no longer infer OpenCode enablement from a duplicate directory alone"
    );
}
