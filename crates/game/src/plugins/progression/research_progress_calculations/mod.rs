use super::research_types::ResearchProject;

pub(crate) fn advance_research_project_by_one_tick(project: &mut ResearchProject) -> bool {
    if project.elapsed_ticks >= project.required_ticks {
        return true;
    }
    project.elapsed_ticks += 1;
    project.elapsed_ticks == project.required_ticks
}
