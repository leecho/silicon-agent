//! Project runtime skill resolution shared by engine and UI commands.

use std::collections::BTreeMap;

use crate::project::ProjectService;
use crate::skill::types::SkillSummary;
use crate::skill::SkillService;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSkillSummary {
    pub skill: SkillSummary,
    /// "team" | "expert"
    pub source_kind: String,
    pub source_id: String,
    pub source_name: String,
}

/// Skills that are actually available to a project run through project membership.
///
/// This intentionally mirrors the engine's project branch: imported team snapshots
/// contribute their source team's private skills, and all project members contribute
/// their expert private skills. Global/plugin skills are not included here because
/// they are not project-dedicated.
pub fn list_project_runtime_skills(
    projects: &ProjectService,
    skills: &SkillService,
    project_id: &str,
) -> Result<Vec<SkillSummary>, String> {
    let mut by_id = BTreeMap::<String, SkillSummary>::new();

    for team_id in projects.origin_team_ids(project_id)? {
        for skill in skills.list_enabled_by_team(&team_id)? {
            by_id.entry(skill.id.clone()).or_insert(skill);
        }
    }

    for expert_name in projects.member_expert_names(project_id)? {
        for skill in skills.list_enabled_by_expert(&expert_name)? {
            by_id.entry(skill.id.clone()).or_insert(skill);
        }
    }

    let mut out: Vec<SkillSummary> = by_id.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(out)
}

pub fn list_project_runtime_skill_items<F>(
    projects: &ProjectService,
    skills: &SkillService,
    team_name: F,
    project_id: &str,
) -> Result<Vec<ProjectSkillSummary>, String>
where
    F: Fn(&str) -> Option<String>,
{
    let members = projects.list_members(project_id)?;
    let mut by_id = BTreeMap::<String, ProjectSkillSummary>::new();

    for team_id in projects.origin_team_ids(project_id)? {
        let source_name = team_name(&team_id).unwrap_or_else(|| team_id.clone());
        for skill in skills.list_enabled_by_team(&team_id)? {
            by_id.entry(skill.id.clone()).or_insert(ProjectSkillSummary {
                skill,
                source_kind: "team".into(),
                source_id: team_id.clone(),
                source_name: source_name.clone(),
            });
        }
    }

    for member in members {
        let expert_name = member.expert_name;
        if expert_name.trim().is_empty() {
            continue;
        }
        let source_name = member
            .display_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| expert_name.clone());
        for skill in skills.list_enabled_by_expert(&expert_name)? {
            by_id.entry(skill.id.clone()).or_insert(ProjectSkillSummary {
                skill,
                source_kind: "expert".into(),
                source_id: expert_name.clone(),
                source_name: source_name.clone(),
            });
        }
    }

    let mut out: Vec<ProjectSkillSummary> = by_id.into_values().collect();
    out.sort_by(|a, b| a.skill.name.cmp(&b.skill.name).then(a.skill.id.cmp(&b.skill.id)));
    Ok(out)
}
