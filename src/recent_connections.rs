use crate::project::{Endpoint, Project};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const LIMIT: usize = 12;
#[derive(Default)]
pub struct RecentConnections {
    pub entries: Vec<Project>,
}
pub struct RecentGroup {
    pub source_address: String,
    pub target_address: String,
    pub indices: Vec<usize>,
}
fn address(endpoint: &Endpoint) -> String {
    if endpoint.snapshot {
        format!("Snapshot · {}", endpoint.path)
    } else {
        endpoint.connection.server.trim().to_owned()
    }
}
pub fn history_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("SchemaCompare")
        .join("recent-connections.json")
}
fn ready(e: &Endpoint) -> bool {
    if e.snapshot {
        !e.path.trim().is_empty()
    } else {
        !e.connection.server.trim().is_empty() && !e.connection.database.trim().is_empty()
    }
}
fn same_endpoint(a: &Endpoint, b: &Endpoint) -> bool {
    if a.snapshot != b.snapshot {
        return false;
    }
    if a.snapshot {
        a.path == b.path
    } else {
        a.connection.server == b.connection.server
            && a.connection.database == b.connection.database
            && a.connection.windows_auth == b.connection.windows_auth
            && a.connection.username == b.connection.username
            && a.connection.trust_certificate == b.connection.trust_certificate
    }
}
impl RecentConnections {
    /// Keep groups and their database pairs in most-recently-used order.
    pub fn groups(&self) -> Vec<RecentGroup> {
        self.filtered_groups("")
    }
    pub fn filtered_groups(&self, query: &str) -> Vec<RecentGroup> {
        let mut groups = Vec::<RecentGroup>::new();
        let query = query.to_lowercase();
        for (index, project) in self.entries.iter().enumerate() {
            let searchable = [
                address(&project.source),
                address(&project.target),
                project.source.connection.database.clone(),
                project.target.connection.database.clone(),
                project.source.connection.username.clone(),
                project.target.connection.username.clone(),
            ]
            .join("\n")
            .to_lowercase();
            if !query
                .split_whitespace()
                .all(|term| searchable.contains(term))
            {
                continue;
            }
            let source_address = address(&project.source);
            let target_address = address(&project.target);
            let same_address = |a: &str, b: &str, snapshot: bool| {
                if snapshot {
                    a == b
                } else {
                    a.eq_ignore_ascii_case(b)
                }
            };
            if let Some(group) = groups.iter_mut().find(|g| {
                same_address(&g.source_address, &source_address, project.source.snapshot)
                    && same_address(&g.target_address, &target_address, project.target.snapshot)
            }) {
                group.indices.push(index);
            } else {
                groups.push(RecentGroup {
                    source_address,
                    target_address,
                    indices: vec![index],
                });
            }
        }
        groups
    }
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let entries: Vec<Project> =
            serde_json::from_str(&text).context("Read recent connections")?;
        let mut clean = Vec::<Project>::new();
        for p in entries {
            if ready(&p.source)
                && ready(&p.target)
                && !clean.iter().any(|old| {
                    same_endpoint(&old.source, &p.source) && same_endpoint(&old.target, &p.target)
                })
            {
                clean.push(p);
            }
            if clean.len() == LIMIT {
                break;
            }
        }
        Ok(Self { entries: clean })
    }
    pub fn remember(&mut self, project: &Project, path: &Path) -> Result<()> {
        if !ready(&project.source) || !ready(&project.target) {
            return Ok(());
        }
        self.entries.retain(|p| {
            !same_endpoint(&p.source, &project.source) || !same_endpoint(&p.target, &project.target)
        });
        self.entries.insert(0, project.clone());
        self.entries.truncate(LIMIT);
        write_entries(path, &self.entries)
    }
    pub fn clear(&mut self, path: &Path) -> Result<()> {
        write_entries(path, &[])?;
        self.entries.clear();
        Ok(())
    }
}
fn write_entries(path: &Path, entries: &[Project]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).context("Create recent connections folder")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    // Connection.password is serde(skip), so credentials stay only in memory.
    serde_json::to_writer_pretty(&mut temp, entries)?;
    temp.as_file().sync_all()?;
    temp.persist(path).context("Save recent connections")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project(db: &str) -> Project {
        let mut p = Project::default();
        for e in [&mut p.source, &mut p.target] {
            e.connection.server = "localhost".into();
            e.connection.database = db.into();
            e.connection.windows_auth = false;
            e.connection.username = "testuser".into();
            e.connection.password = "private-password-marker".into();
        }
        p
    }
    #[test]
    fn groups_by_ordered_server_pair_not_database_or_login_and_preserves_entry_indices() {
        let mut a = project("Newest");
        a.source.connection.server = "SOURCE,15433".into();
        a.target.connection.server = "TARGET,15433".into();
        let mut b = a.clone();
        b.source.connection.server = "Other,15433".into();
        let mut c = a.clone();
        c.source.connection.server = "source,15433".into();
        c.source.connection.database = "Older".into();
        c.target.connection.username = "another-login".into();
        let mut d = a.clone();
        std::mem::swap(
            &mut d.source.connection.server,
            &mut d.target.connection.server,
        );
        let history = RecentConnections {
            entries: vec![a, b, c, d],
        };
        let groups = history.groups();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].indices, vec![0, 2]);
        assert_eq!(groups[1].indices, vec![1]);
        assert_eq!(groups[2].indices, vec![3]);
        assert_eq!(
            history.entries[groups[0].indices[1]]
                .source
                .connection
                .database,
            "Older"
        );
    }
    #[test]
    fn search_filters_entries_without_changing_original_selection_indices_or_history() {
        let mut a = project("Alpha");
        a.source.connection.server = "SourceSQL,15433".into();
        a.target.connection.server = "TargetSQL,15433".into();
        let mut b = a.clone();
        b.source.connection.database = "Budget".into();
        let history = RecentConnections {
            entries: vec![a, b],
        };
        let groups = history.filtered_groups("  SOURCESQL budget  ");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].indices, vec![1]);
        assert_eq!(history.filtered_groups("targetsql")[0].indices, vec![0, 1]);
        assert!(history.filtered_groups("missing").is_empty());
        assert!(history
            .filtered_groups("private-password-marker")
            .is_empty());
        assert_eq!(history.entries.len(), 2);
        assert_eq!(history.filtered_groups(" ")[0].indices, vec![0, 1]);
    }
    #[test]
    fn history_survives_reload_without_persisting_passwords() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut history = RecentConnections::default();
        history.remember(&project("App"), &path).unwrap();
        assert_eq!(
            history.entries[0].source.connection.password,
            "private-password-marker"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("password"));
        assert!(!text.contains("private-password-marker"));
        let loaded = RecentConnections::load(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert!(loaded.entries[0].source.connection.password.is_empty());
        assert_eq!(loaded.entries[0].target.connection.database, "App");
    }
    #[test]
    fn remembers_most_recent_pair_once_and_keeps_twelve() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut history = RecentConnections::default();
        for i in 0..15 {
            history
                .remember(&project(&format!("Db{i}")), &path)
                .unwrap();
        }
        let mut p = project("Db10");
        p.source.connection.password = "changed".into();
        history.remember(&p, &path).unwrap();
        assert_eq!(history.entries.len(), 12);
        assert_eq!(history.entries[0].source.connection.database, "Db10");
        assert_eq!(
            history
                .entries
                .iter()
                .filter(|p| p.source.connection.database == "Db10")
                .count(),
            1
        );
        assert_eq!(RecentConnections::load(&path).unwrap().entries.len(), 12);
    }
    #[test]
    fn clear_removes_persisted_and_in_memory_history_but_not_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut history = RecentConnections::default();
        let p = project("App");
        history.remember(&p, &path).unwrap();
        history.clear(&path).unwrap();
        assert!(history.entries.is_empty());
        assert!(RecentConnections::load(&path).unwrap().entries.is_empty());
        assert_eq!(p.source.connection.database, "App");
    }
    #[test]
    fn incomplete_connections_are_not_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut history = RecentConnections::default();
        history.remember(&Project::default(), &path).unwrap();
        assert!(!path.exists());
    }
}
