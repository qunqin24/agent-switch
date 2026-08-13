use super::*;
use tempfile::tempdir;

fn write_skill(dir: &Path, name: &str) {
    fs::create_dir_all(dir).expect("create skill dir");
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Test skill\n---\n"),
    )
    .expect("write SKILL.md");
}

fn discoverable_skill() -> DiscoverableSkill {
    DiscoverableSkill {
        key: "vercel-labs/skills:find-skills".to_string(),
        name: "find-skills".to_string(),
        description: "Find agent skills".to_string(),
        directory: "find-skills".to_string(),
        readme_url: None,
        repo_owner: "vercel-labs".to_string(),
        repo_name: "skills".to_string(),
        repo_branch: "main".to_string(),
    }
}

#[test]
fn skills_cli_add_args_use_non_interactive_global_install() {
    let args = SkillService::skills_cli_add_args(&discoverable_skill(), "find-skills", true)
        .expect("valid skills CLI arguments");

    assert_eq!(
        args,
        vec![
            "add",
            "vercel-labs/skills",
            "--skill",
            "find-skills",
            "--global",
            "--agent",
            "codex",
            "--yes",
        ]
    );
}

#[test]
fn skills_cli_update_args_use_non_interactive_global_update() {
    let args = SkillService::skills_cli_update_args("find-skills", true)
        .expect("valid skills CLI arguments");

    assert_eq!(args, vec!["update", "find-skills", "--global", "--yes"]);
}

#[test]
fn skills_cli_args_reject_shell_metacharacters() {
    let mut skill = discoverable_skill();
    skill.repo_owner = "vercel-labs;touch-pwned".to_string();

    assert!(SkillService::skills_cli_add_args(&skill, "find-skills", false).is_err());
    assert!(SkillService::skills_cli_update_args("find-skills && bad", false).is_err());
}

#[test]
fn skills_cli_project_args_keep_cli_workspaces_isolated() {
    let add = SkillService::skills_cli_add_args(&discoverable_skill(), "find-skills", false)
        .expect("valid project install arguments");
    let update = SkillService::skills_cli_update_args("find-skills", false)
        .expect("valid project update arguments");

    assert!(!add.iter().any(|argument| argument == "--global"));
    assert!(!update.iter().any(|argument| argument == "--global"));
    assert_eq!(
        add,
        vec![
            "add",
            "vercel-labs/skills",
            "--skill",
            "find-skills",
            "--agent",
            "codex",
            "--yes",
        ]
    );
    assert_eq!(update, vec!["update", "find-skills", "--yes"]);
}

#[test]
fn parse_agents_lock_at_reads_global_command_lock() {
    let temp = tempdir().expect("tempdir");
    let agents_dir = temp.path().join(".agents");
    fs::create_dir_all(&agents_dir).expect("create agents dir");
    fs::write(
        agents_dir.join(".skill-lock.json"),
        r#"{
              "skills": {
                "find-skills": {
                  "source": "vercel-labs/skills",
                  "sourceType": "github",
                  "skillPath": "skills/find-skills/SKILL.md",
                  "branch": "main"
                }
              }
            }"#,
    )
    .expect("write lock file");

    let lock = parse_agents_lock_at(temp.path());
    let entry = lock.get("find-skills").expect("parse isolated lock");
    assert_eq!(entry.owner, "vercel-labs");
    assert_eq!(entry.repo, "skills");
    assert_eq!(entry.branch.as_deref(), Some("main"));
}

#[test]
fn parse_skills_cli_lock_reads_project_workspace_lock() {
    let temp = tempdir().expect("tempdir");
    fs::write(
        temp.path().join("skills-lock.json"),
        r#"{
              "version": 1,
              "skills": {
                "find-skills": {
                  "source": "vercel-labs/skills",
                  "sourceType": "github",
                  "skillPath": "skills/find-skills/SKILL.md"
                }
              }
            }"#,
    )
    .expect("write project lock file");

    let lock = parse_skills_cli_lock(temp.path(), false);
    let entry = lock.get("find-skills").expect("parse project lock");
    assert_eq!(entry.owner, "vercel-labs");
    assert_eq!(entry.repo, "skills");
    assert_eq!(
        entry.skill_path.as_deref(),
        Some("skills/find-skills/SKILL.md")
    );
}

#[test]
fn resolve_skill_source_dir_returns_repo_root_for_root_level_skill() {
    let temp = tempdir().expect("tempdir");
    write_skill(temp.path(), "Root Skill");

    let resolved = SkillService::resolve_skill_source_dir(temp.path(), "last30days-skill-cn")
        .expect("root-level skill should resolve to the extracted repo root");

    assert_eq!(resolved, temp.path());
}

#[test]
fn resolve_skill_source_dir_returns_direct_nested_directory_when_present() {
    let temp = tempdir().expect("tempdir");
    let nested = temp.path().join("skills").join("nested-skill");
    write_skill(&nested, "Nested Skill");

    let resolved = SkillService::resolve_skill_source_dir(temp.path(), "skills/nested-skill")
        .expect("nested skill should resolve from its relative source path");

    assert_eq!(resolved, nested);
}

#[test]
fn resolve_skill_source_dir_falls_back_to_matching_install_name() {
    let temp = tempdir().expect("tempdir");
    let nested = temp.path().join("skills").join("nested-skill");
    write_skill(&nested, "Nested Skill");

    let resolved = SkillService::resolve_skill_source_dir(temp.path(), "nested-skill")
        .expect("install name should fall back to the matching discovered skill directory");

    assert_eq!(resolved, nested);
}

#[test]
fn replace_dest_with_copy_rejects_empty_source_without_touching_existing_dest() {
    let temp = tempdir().expect("tempdir");
    let source = temp.path().join("source-skill");
    let dest = temp.path().join("app-skills").join("source-skill");
    fs::create_dir_all(&source).expect("create empty source");
    write_skill(&dest, "Existing Skill");

    let err = SkillService::replace_dest_with_copy(&source, &dest, "source-skill")
        .expect_err("empty source should not replace existing app skill");

    assert!(
        err.to_string().contains("SKILL.md"),
        "unexpected error: {err:#}"
    );
    assert!(
        dest.join("SKILL.md").is_file(),
        "existing destination skill should be preserved"
    );
}

#[test]
fn map_skills_sh_skill_builds_canonical_detail_url() {
    let skill = map_skills_sh_skill(SkillsShApiSkill {
        id: "vercel-labs/skills/find-skills".to_string(),
        skill_id: "find-skills".to_string(),
        name: "find-skills".to_string(),
        installs: 42,
        source: "vercel-labs/skills".to_string(),
    })
    .expect("GitHub skills.sh entry should be installable");

    assert_eq!(skill.repo_owner, "vercel-labs");
    assert_eq!(skill.repo_name, "skills");
    assert_eq!(
        skill.detail_url,
        "https://skills.sh/vercel-labs/skills/find-skills"
    );
    assert_eq!(skill.readme_url.as_deref(), Some(skill.detail_url.as_str()));
}

#[test]
fn map_skills_sh_skill_filters_well_known_sources() {
    let skill = map_skills_sh_skill(SkillsShApiSkill {
        id: "mintlify.com/mintlify".to_string(),
        skill_id: "mintlify".to_string(),
        name: "Mintlify".to_string(),
        installs: 42,
        source: "mintlify.com".to_string(),
    });

    assert!(skill.is_none());
}

#[test]
fn parse_skills_sh_leaderboard_reads_public_next_payload() {
    let payload = r#"4a:["$","component",null,{"initialSkills":[{"source":"vercel-labs/skills","skillId":"find-skills","name":"find-skills","installs":24531,"weeklyInstalls":[120,140,130],"isOfficial":true}],"totalSkills":8420,"allTimeTotal":1044078,"view":"all-time"}]
next"#;
    let encoded = serde_json::to_string(payload).expect("encode RSC payload");
    let html = format!(r#"<html><script>self.__next_f.push([1,{encoded}])</script></html>"#);

    let props = parse_skills_sh_leaderboard(&html).expect("parse leaderboard payload");

    assert_eq!(props.view, "all-time");
    assert_eq!(props.total_skills, 8420);
    assert_eq!(props.all_time_total, 1_044_078);
    assert_eq!(props.initial_skills.len(), 1);
    assert_eq!(props.initial_skills[0].skill_id, "find-skills");
    assert_eq!(props.initial_skills[0].weekly_installs, vec![120, 140, 130]);
    assert!(props.initial_skills[0].is_official);
}

#[test]
fn parse_skills_sh_detail_reads_public_page_sections() {
    let html = r#"
<a href="/topic/agent-workflows">Agent workflows</a>
<div class="uppercase">Summary</div>
<div><div><div class="prose detail"><p><strong>Summary text</strong></p></div></div></div><div class="bg-background">
<span>SKILL.md</span>
<div class="prose detail"><h1>Find Skills</h1><p>Body text</p></div><div class="relative">
<span>GitHub Stars</span><div><svg></svg><span>27.6K</span></div>
<span>First Seen</span><div class="value">Jan 26, 2026</div>
<div>Security Audits</div>
<a href="/owner/repo/skill/security/socket"><span class="name">Socket</span><span class="status">Pass</span></a>
<a href="/owner/repo/skill/security/snyk"><span class="name">Snyk</span><span class="status">Warn</span></a>
</div></div></div></main>
"#;

    let detail = parse_skills_sh_detail(html).expect("parse detail");

    assert_eq!(detail.topic.as_deref(), Some("Agent workflows"));
    assert_eq!(detail.summary_html, "<p><strong>Summary text</strong></p>");
    assert_eq!(detail.content_html, "<h1>Find Skills</h1><p>Body text</p>");
    assert_eq!(detail.github_stars.as_deref(), Some("27.6K"));
    assert_eq!(detail.first_seen.as_deref(), Some("Jan 26, 2026"));
    assert_eq!(detail.security_audits.len(), 2);
    assert_eq!(detail.security_audits[0].provider, "Socket");
    assert_eq!(detail.security_audits[1].status, "Warn");
}

#[test]
fn parse_skills_sh_publisher_reads_sources_and_metrics() {
    let html = r#"
<div><svg></svg>49<!-- --> <!-- -->sources</span></div>
<div><svg></svg>198<!-- --> skills</span></div>
<div><svg></svg>5.3M<!-- --> <!-- -->total installs</span></div>
<a class="group grid" href="/vercel-labs/skills"><div><h3>skills</h3><p>1<!-- --> <!-- -->skill<!-- -->:<!-- --> <!-- -->find-skills</p></div><div><span>2.8M</span></div></a>
<a class="group grid" href="/vercel-labs/agent-browser"><div><h3>agent-browser</h3><p>2<!-- --> <!-- -->skills<!-- -->:<!-- --> <!-- -->agent-browser, derive-client</p></div><div><span>531.3K</span></div></a>
"#;

    let detail = parse_skills_sh_publisher(html, "vercel-labs").expect("parse publisher page");

    assert_eq!(detail.owner, "vercel-labs");
    assert_eq!(detail.source_count, 49);
    assert_eq!(detail.skill_count, 198);
    assert_eq!(detail.total_installs, "5.3M");
    assert_eq!(detail.sources.len(), 2);
    assert_eq!(detail.sources[0].name, "skills");
    assert_eq!(detail.sources[0].skill_summary, "1 skill: find-skills");
    assert_eq!(detail.sources[1].installs, "531.3K");
}

#[test]
fn parse_skills_sh_repository_reads_skills_and_metrics() {
    let html = r#"
<div><svg></svg>1<!-- --> <!-- -->skill</span></div>
<div><svg></svg>2.8M<!-- --> <!-- -->total installs</span></div>
<a class="group grid" href="/vercel-labs/skills/find-skills"><div><h3>find-skills</h3></div><div><span>2.8M</span></div></a>
"#;

    let detail =
        parse_skills_sh_repository(html, "vercel-labs", "skills").expect("parse repository page");

    assert_eq!(detail.owner, "vercel-labs");
    assert_eq!(detail.repository, "skills");
    assert_eq!(detail.skill_count, 1);
    assert_eq!(detail.total_installs, "2.8M");
    assert_eq!(detail.skills.len(), 1);
    assert_eq!(detail.skills[0].skill_id, "find-skills");
    assert_eq!(detail.skills[0].installs, 2_800_000);
}
