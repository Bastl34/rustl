use std::fs::File;
use serde::{Deserialize, Serialize};
use crate::helper::file::normalize_path_separators;

const RECENT_PROJECTS_FILE: &str = "data/recent_projects.json";

#[derive(Serialize, Deserialize)]
pub struct RecentProjectsData
{
    pub projects: Vec<String>
}

impl RecentProjectsData
{
    pub fn new() -> Self
    {
        let mut projects = Self
        {
            projects: Vec::new()
        };

        projects.load();
        projects
    }

    pub fn load(&mut self)
    {
        if let Ok(file) = File::open(RECENT_PROJECTS_FILE)
        {
            if let Ok(data) = serde_json::from_reader(file)
            {
                *self = data;
            }
        }
    }

    pub fn add_and_save(&mut self, project_path: String)
    {
        let project_path = normalize_path_separators(&project_path);

        // check if already exists and remove it to move it to the end of the list
        self.projects.retain(|x| *x != project_path);

        // add to the end of the list
        self.projects.push(project_path);

        if let Ok(file) = File::create(RECENT_PROJECTS_FILE)
        {
            let _ = serde_json::to_writer_pretty(file, self);
        }
    }

    pub fn get_latest_items(&self, amount: usize) -> Vec<String>
    {
        self.projects.iter().rev().take(amount).cloned().collect()
    }
}