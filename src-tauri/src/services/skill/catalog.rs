use super::*;

impl SkillService {
    // ========== 仓库管理（保留原有逻辑）==========

    /// 列出仓库
    pub fn list_repos(&self, store: &SkillStore) -> Vec<SkillRepo> {
        store.repos.clone()
    }

    /// 添加仓库
    pub fn add_repo(&self, store: &mut SkillStore, repo: SkillRepo) -> Result<()> {
        if let Some(pos) = store
            .repos
            .iter()
            .position(|r| r.owner == repo.owner && r.name == repo.name)
        {
            store.repos[pos] = repo;
        } else {
            store.repos.push(repo);
        }

        Ok(())
    }

    /// 删除仓库
    pub fn remove_repo(&self, store: &mut SkillStore, owner: String, name: String) -> Result<()> {
        store
            .repos
            .retain(|r| !(r.owner == owner && r.name == name));

        Ok(())
    }

    // ========== skills.sh 搜索 ==========

    /// 搜索 skills.sh 公共目录
    pub async fn search_skills_sh(query: &str, limit: usize) -> Result<SkillsShSearchResult> {
        let query = query.trim();
        if query.chars().count() < 2 {
            return Err(anyhow!("skills.sh 搜索关键词至少需要 2 个字符"));
        }

        let client = crate::proxy::http_client::get();
        let limit = limit.clamp(1, 200);

        let url = url::Url::parse_with_params(
            "https://skills.sh/api/search",
            &[("q", query), ("limit", &limit.to_string())],
        )?;

        let resp = client
            .get(url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?
            .error_for_status()?
            .json::<SkillsShApiResponse>()
            .await?;

        let skills = resp
            .skills
            .into_iter()
            .filter_map(map_skills_sh_skill)
            .collect::<Vec<_>>();
        let result_count = skills.len();

        Ok(SkillsShSearchResult {
            skills,
            result_count,
            query: resp.query,
        })
    }

    /// 读取 skills.sh 公开榜单。
    ///
    /// `/api/v1/skills` 要求 Vercel OIDC；桌面端改读官网公开页面中用于首屏渲染的
    /// `initialSkills` 数据，确保用户无需配置第三方凭据。
    pub async fn get_skills_sh_leaderboard(
        view: &str,
        limit: usize,
    ) -> Result<SkillsShLeaderboardResult> {
        let path = match view {
            "all-time" => "",
            "trending" => "trending",
            "hot" => "hot",
            _ => return Err(anyhow!("Unsupported skills.sh leaderboard view: {view}")),
        };
        let url = if path.is_empty() {
            "https://www.skills.sh/".to_string()
        } else {
            format!("https://www.skills.sh/{path}")
        };

        let html = crate::proxy::http_client::get()
            .get(url)
            .header(reqwest::header::ACCEPT, "text/html")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let props = parse_skills_sh_leaderboard(&html)?;
        if props.view != view {
            return Err(anyhow!(
                "skills.sh leaderboard returned '{}' for requested view '{view}'",
                props.view
            ));
        }

        let skills = props
            .initial_skills
            .into_iter()
            .filter_map(map_skills_sh_leaderboard_skill)
            .take(limit.clamp(1, 300))
            .collect::<Vec<_>>();
        let result_count = skills.len();

        Ok(SkillsShLeaderboardResult {
            skills,
            result_count,
            total_skills: props.total_skills,
            all_time_total: props.all_time_total,
            view: props.view,
        })
    }

    /// 读取 skills.sh 的公开发布者详情页。
    pub async fn get_skills_sh_publisher(owner: &str) -> Result<SkillsShPublisherDetail> {
        validate_skills_sh_segment("publisher", owner)?;
        let html = fetch_skills_sh_page(&[owner]).await?;
        parse_skills_sh_publisher(&html, owner)
    }

    /// 读取 skills.sh 的公开仓库详情页。
    pub async fn get_skills_sh_repository(
        owner: &str,
        repository: &str,
    ) -> Result<SkillsShRepositoryDetail> {
        validate_skills_sh_segment("publisher", owner)?;
        validate_skills_sh_segment("repository", repository)?;
        let html = fetch_skills_sh_page(&[owner, repository]).await?;
        parse_skills_sh_repository(&html, owner, repository)
    }

    /// 读取 skills.sh 的公开 Skill 详情页。
    pub async fn get_skills_sh_detail(
        repo_owner: &str,
        repo_name: &str,
        skill_id: &str,
    ) -> Result<SkillsShSkillDetail> {
        for (label, segment) in [
            ("repo owner", repo_owner),
            ("repo name", repo_name),
            ("skill id", skill_id),
        ] {
            validate_skills_sh_segment(label, segment)?;
        }

        let html = fetch_skills_sh_page(&[repo_owner, repo_name, skill_id]).await?;

        parse_skills_sh_detail(&html)
    }
}

pub(super) fn validate_skills_sh_segment(label: &str, segment: &str) -> Result<()> {
    if segment.is_empty() || segment == "." || segment == ".." || segment.contains(['/', '\\']) {
        return Err(anyhow!("Invalid skills.sh {label}: {segment}"));
    }
    Ok(())
}

pub(super) async fn fetch_skills_sh_page(segments: &[&str]) -> Result<String> {
    let mut url = url::Url::parse("https://www.skills.sh/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("Failed to build skills.sh URL"))?
        .extend(segments.iter().copied());

    crate::proxy::http_client::get()
        .get(url)
        .header(reqwest::header::ACCEPT, "text/html")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
        .context("Failed to read skills.sh page")
}

pub(super) fn extract_skills_sh_metric(html: &str, label: &str) -> Option<String> {
    let mut cursor = 0;

    while let Some(svg_end) = html[cursor..].find("</svg>") {
        let value_start = cursor + svg_end + "</svg>".len();
        let after_icon = &html[value_start..];
        let Some(span_end) = after_icon.find("</span>") else {
            break;
        };
        let visible_text = strip_html_tags(&after_icon[..span_end]);

        if let Some(value) = visible_text.strip_suffix(label) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }

        cursor = value_start;
    }

    None
}

pub(super) fn extract_skills_sh_count(html: &str, singular: &str) -> Option<usize> {
    let value = extract_skills_sh_metric(html, &format!("{singular}s"))
        .or_else(|| extract_skills_sh_metric(html, singular))?;
    value.replace(',', "").parse().ok()
}

pub(super) fn parse_compact_installs(value: &str) -> Option<u64> {
    let normalized = value.trim().replace(',', "");
    let (number, multiplier) = match normalized.chars().last()? {
        'K' | 'k' => (&normalized[..normalized.len() - 1], 1_000_f64),
        'M' | 'm' => (&normalized[..normalized.len() - 1], 1_000_000_f64),
        'B' | 'b' => (&normalized[..normalized.len() - 1], 1_000_000_000_f64),
        _ => (normalized.as_str(), 1_f64),
    };
    number
        .parse::<f64>()
        .ok()
        .map(|number| (number * multiplier).round() as u64)
}

pub(super) fn parse_skills_sh_publisher(
    html: &str,
    owner: &str,
) -> Result<SkillsShPublisherDetail> {
    let owner_pattern = regex::escape(owner);
    let row_pattern = format!(
        r#"(?s)<a[^>]+href="/{owner_pattern}/([^"/]+)"[^>]*>.*?<h3[^>]*>(.*?)</h3>.*?<p[^>]*>(.*?)</p>.*?<span[^>]*>(.*?)</span>.*?</a>"#
    );
    let row_regex = Regex::new(&row_pattern).context("Failed to build publisher row parser")?;
    let sources = row_regex
        .captures_iter(html)
        .filter_map(|captures| {
            let name = strip_html_tags(captures.get(1)?.as_str());
            let heading = strip_html_tags(captures.get(2)?.as_str());
            if name != heading {
                return None;
            }
            Some(SkillsShSourceSummary {
                name,
                skill_summary: strip_html_tags(captures.get(3)?.as_str()),
                installs: strip_html_tags(captures.get(4)?.as_str()),
            })
        })
        .collect::<Vec<_>>();

    if sources.is_empty() {
        return Err(anyhow!("skills.sh publisher sources were not found"));
    }

    Ok(SkillsShPublisherDetail {
        owner: owner.to_string(),
        source_count: extract_skills_sh_count(html, "source").unwrap_or(sources.len()),
        skill_count: extract_skills_sh_count(html, "skill")
            .ok_or_else(|| anyhow!("skills.sh publisher skill count was not found"))?,
        total_installs: extract_skills_sh_metric(html, "total installs")
            .ok_or_else(|| anyhow!("skills.sh publisher installs were not found"))?,
        sources,
    })
}

pub(super) fn parse_skills_sh_repository(
    html: &str,
    owner: &str,
    repository: &str,
) -> Result<SkillsShRepositoryDetail> {
    let owner_pattern = regex::escape(owner);
    let repository_pattern = regex::escape(repository);
    let row_pattern = format!(
        r#"(?s)<a[^>]+href="/{owner_pattern}/{repository_pattern}/([^"/]+)"[^>]*>.*?<h3[^>]*>(.*?)</h3>.*?<span[^>]*>(.*?)</span>.*?</a>"#
    );
    let row_regex = Regex::new(&row_pattern).context("Failed to build repository row parser")?;
    let skills = row_regex
        .captures_iter(html)
        .filter_map(|captures| {
            let skill_id = strip_html_tags(captures.get(1)?.as_str());
            let name = strip_html_tags(captures.get(2)?.as_str());
            let installs_label = strip_html_tags(captures.get(3)?.as_str());
            Some(SkillsShRepositorySkill {
                skill_id,
                name,
                installs: parse_compact_installs(&installs_label)?,
                installs_label,
            })
        })
        .collect::<Vec<_>>();

    if skills.is_empty() {
        return Err(anyhow!("skills.sh repository skills were not found"));
    }

    Ok(SkillsShRepositoryDetail {
        owner: owner.to_string(),
        repository: repository.to_string(),
        skill_count: extract_skills_sh_count(html, "skill").unwrap_or(skills.len()),
        total_installs: extract_skills_sh_metric(html, "total installs")
            .ok_or_else(|| anyhow!("skills.sh repository installs were not found"))?,
        skills,
    })
}

pub(super) fn extract_prose_section<'a>(
    html: &'a str,
    label: &str,
    end_marker: &str,
) -> Option<&'a str> {
    let label_index = html.find(label)?;
    let after_label = &html[label_index + label.len()..];
    let prose_index = after_label.find("<div class=\"prose ")?;
    let prose = &after_label[prose_index..];
    let content_start = prose.find('>')? + 1;
    let content = &prose[content_start..];
    let content_end = content.find(end_marker)?;
    Some(content[..content_end].trim())
}

pub(super) fn decode_html_text(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&#x3C;", "<")
        .replace("&gt;", ">")
}

pub(super) fn strip_html_tags(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(character),
            _ => {}
        }
    }
    decode_html_text(result.trim())
}

pub(super) fn extract_element_text_after(html: &str, marker: &str, tag: &str) -> Option<String> {
    let marker_index = html.find(marker)?;
    let after_marker = &html[marker_index + marker.len()..];
    let tag_prefix = format!("<{tag}");
    let tag_index = after_marker.find(&tag_prefix)?;
    let tag_content = &after_marker[tag_index..];
    let content_start = tag_content.find('>')? + 1;
    let content = &tag_content[content_start..];
    let end_tag = format!("</{tag}>");
    let content_end = content.find(&end_tag)?;
    Some(strip_html_tags(&content[..content_end]))
}

pub(super) fn parse_skills_sh_security_audits(html: &str) -> Vec<SkillsShSecurityAudit> {
    let Some(section_index) = html.find(">Security Audits</div>") else {
        return Vec::new();
    };
    let section_tail = &html[section_index..];
    let section_end = section_tail
        .find("</div></div></div></main>")
        .unwrap_or(section_tail.len());
    let mut section = &section_tail[..section_end];
    let mut audits = Vec::new();

    while let Some(security_index) = section.find("/security/") {
        let after_security = &section[security_index..];
        let Some(anchor_end) = after_security.find("</a>") else {
            break;
        };
        let anchor = &after_security[..anchor_end];
        let mut spans = Vec::new();
        let mut rest = anchor;
        while let Some(span_index) = rest.find("<span") {
            let span = &rest[span_index..];
            let Some(content_start) = span.find('>') else {
                break;
            };
            let content = &span[content_start + 1..];
            let Some(content_end) = content.find("</span>") else {
                break;
            };
            spans.push(strip_html_tags(&content[..content_end]));
            rest = &content[content_end + "</span>".len()..];
        }
        if spans.len() >= 2 && !spans[0].is_empty() && !spans[1].is_empty() {
            audits.push(SkillsShSecurityAudit {
                provider: spans[0].clone(),
                status: spans[1].clone(),
            });
        }
        section = &after_security[anchor_end + "</a>".len()..];
    }

    audits
}

pub(super) fn parse_skills_sh_detail(html: &str) -> Result<SkillsShSkillDetail> {
    const SUMMARY_LABEL: &str = ">Summary</div>";
    const SUMMARY_END: &str = "</div></div></div><div class=\"bg-background\">";
    const CONTENT_LABEL: &str = "<span>SKILL.md</span>";
    const CONTENT_END: &str = "</div><div class=\"relative\">";

    let summary_html = extract_prose_section(html, SUMMARY_LABEL, SUMMARY_END)
        .ok_or_else(|| anyhow!("skills.sh detail summary was not found"))?;
    let content_html = extract_prose_section(html, CONTENT_LABEL, CONTENT_END)
        .ok_or_else(|| anyhow!("skills.sh detail content was not found"))?;
    let topic = html.find("href=\"/topic/").and_then(|topic_index| {
        let topic_anchor = &html[topic_index..];
        let content_start = topic_anchor.find('>')? + 1;
        let content = &topic_anchor[content_start..];
        let content_end = content.find("</a>")?;
        Some(strip_html_tags(&content[..content_end]))
    });

    Ok(SkillsShSkillDetail {
        topic,
        summary_html: summary_html.to_string(),
        content_html: content_html.to_string(),
        github_stars: extract_element_text_after(html, ">GitHub Stars</span>", "span"),
        first_seen: extract_element_text_after(html, ">First Seen</span>", "div"),
        security_audits: parse_skills_sh_security_audits(html),
    })
}

pub(super) fn parse_skills_sh_leaderboard(html: &str) -> Result<SkillsShLeaderboardProps> {
    const PAYLOAD_PREFIX: &str = "self.__next_f.push([1,";
    const PAYLOAD_SUFFIX: &str = "])</script>";
    const PROPS_MARKER: &str = "\"initialSkills\":";
    const ESCAPED_PROPS_MARKER: &str = "\\\"initialSkills\\\":";

    let marker_index = html
        .find(ESCAPED_PROPS_MARKER)
        .ok_or_else(|| anyhow!("skills.sh leaderboard data was not found"))?;
    let payload_prefix_index = html[..marker_index]
        .rfind(PAYLOAD_PREFIX)
        .ok_or_else(|| anyhow!("skills.sh leaderboard payload prefix was not found"))?;
    let encoded_start = payload_prefix_index + PAYLOAD_PREFIX.len();
    let encoded_end = html[marker_index..]
        .find(PAYLOAD_SUFFIX)
        .map(|index| marker_index + index)
        .ok_or_else(|| anyhow!("skills.sh leaderboard payload suffix was not found"))?;
    let encoded_payload = &html[encoded_start..encoded_end];
    let payload: String = serde_json::from_str(encoded_payload)
        .context("Failed to decode skills.sh leaderboard payload")?;

    let props_marker_index = payload
        .find(PROPS_MARKER)
        .ok_or_else(|| anyhow!("skills.sh leaderboard props were not found"))?;
    let props_start = payload[..props_marker_index]
        .rfind('{')
        .ok_or_else(|| anyhow!("skills.sh leaderboard props start was not found"))?;
    let mut deserializer = serde_json::Deserializer::from_str(&payload[props_start..]);
    SkillsShLeaderboardProps::deserialize(&mut deserializer)
        .context("Failed to parse skills.sh leaderboard props")
}

pub(super) fn map_skills_sh_leaderboard_skill(
    skill: SkillsShLeaderboardApiSkill,
) -> Option<SkillsShDiscoverableSkill> {
    let weekly_installs = skill.weekly_installs;
    let is_official = skill.is_official;
    let key = format!("{}/{}", skill.source, skill.skill_id);
    map_skills_sh_skill(SkillsShApiSkill {
        id: key,
        skill_id: skill.skill_id,
        name: skill.name,
        installs: skill.installs,
        source: skill.source,
    })
    .map(|mut mapped| {
        mapped.weekly_installs = weekly_installs;
        mapped.is_official = is_official;
        mapped
    })
}

pub(super) fn map_skills_sh_skill(skill: SkillsShApiSkill) -> Option<SkillsShDiscoverableSkill> {
    let (owner, repo) = skill.source.split_once('/')?;
    if owner.is_empty()
        || repo.is_empty()
        || repo.contains('/')
        || owner.contains('.')
        || repo.contains('.')
    {
        // 当前安装器通过 GitHub 仓库下载；skills.sh 的 well-known 来源暂不混入可安装结果。
        return None;
    }

    let detail_url = format!("https://skills.sh/{}/{}/{}", owner, repo, skill.skill_id);

    Some(SkillsShDiscoverableSkill {
        key: skill.id,
        name: skill.name,
        directory: skill.skill_id,
        repo_owner: owner.to_string(),
        repo_name: repo.to_string(),
        repo_branch: "main".to_string(),
        installs: skill.installs,
        weekly_installs: Vec::new(),
        is_official: false,
        readme_url: Some(detail_url.clone()),
        detail_url,
    })
}
