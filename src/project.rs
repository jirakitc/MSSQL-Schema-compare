use crate::{catalog::Connection, engine::Options, model::Snapshot};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub snapshot: bool,
    pub path: String,
    pub connection: Connection,
}
impl Default for Endpoint {
    fn default() -> Self {
        Self {
            snapshot: false,
            path: String::new(),
            connection: Connection {
                windows_auth: false,
                ..Default::default()
            },
        }
    }
}
impl Endpoint {
    pub fn load(&self) -> Result<Snapshot> {
        self.load_with_progress(|_| {})
    }
    pub fn load_with_progress(&self, progress: impl FnMut(&str)) -> Result<Snapshot> {
        self.load_cancellable(progress, &crate::cancellation::CancellationToken::default())
    }
    pub fn load_cancellable(
        &self,
        mut progress: impl FnMut(&str),
        cancel: &crate::cancellation::CancellationToken,
    ) -> Result<Snapshot> {
        cancel.check()?;
        if self.snapshot {
            progress("Reading and validating snapshot file");
            let snapshot = Snapshot::read(Path::new(&self.path))?;
            cancel.check()?;
            Ok(snapshot)
        } else {
            crate::catalog::capture_cancellable(&self.connection, progress, cancel)
        }
    }
    pub fn label(&self) -> String {
        if self.snapshot {
            Path::new(&self.path)
                .file_name()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Snapshot".into())
        } else {
            self.connection.database.clone()
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Project {
    pub source: Endpoint,
    pub target: Endpoint,
    pub options: Options,
    #[serde(default)]
    pub redgate_path: Option<PathBuf>,
    #[serde(default)]
    pub import_notes: Vec<String>,
}
impl Project {
    pub fn read(path: &Path) -> Result<Self> {
        if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("scp"))
        {
            return import_redgate(path, true);
        }
        let mut p: Self = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        for endpoint in [&mut p.source, &mut p.target] {
            if endpoint.snapshot && Path::new(&endpoint.path).is_relative() {
                endpoint.path = path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(&endpoint.path)
                    .display()
                    .to_string();
            }
        }
        // Native files contain no credentials. Re-read only a linked Redgate project
        // whose endpoints still match, so a changed .scp never supplies wrong credentials.
        p.restore_redgate_credentials();
        Ok(p)
    }
    /// Reuse only matching saved endpoints, preserving credentials already in memory.
    pub fn restore_redgate_credentials(&mut self) {
        if let Some(link) = &self.redgate_path {
            if let Ok(saved) = import_redgate(link, true) {
                for to in [&mut self.source, &mut self.target] {
                    if let Some(from) = [&saved.source, &saved.target].into_iter().find(|from| {
                        !to.snapshot
                            && to.connection.password.is_empty()
                            && to.connection.server == from.connection.server
                            && to.connection.database == from.connection.database
                            && to.connection.username == from.connection.username
                            && to.connection.windows_auth == from.connection.windows_auth
                    }) {
                        to.connection.password = from.connection.password.clone();
                    }
                }
            }
        }
    }
    pub fn write(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

fn parse_source(node: roxmltree::Node<'_, '_>) -> Result<Endpoint> {
    if node.attribute("type") != Some("LiveDatabaseSource") {
        bail!("This Redgate data source is not a live SQL Server database. Snapshot/script-folder imports are not supported yet.");
    }
    let field = |name: &str| {
        node.children()
            .find(|n| n.has_tag_name(name))
            .and_then(|n| n.text())
            .unwrap_or("")
            .to_owned()
    };
    let auth = field("AuthenticationType");
    let windows_auth = match auth.as_str() {
        "SqlPassword" => false,
        "Windows" | "WindowsAuthentication" | "Integrated" | "IntegratedSecurity" => true,
        "" => field("Username").is_empty(),
        _ => bail!("Unsupported Redgate authentication type: {auth}"),
    };
    // SQL Compare 14 projects do not persist a certificate-validation option.
    // Legacy imported connections retain that behavior while requesting encryption.
    let c = Connection {
        server: field("ServerName"),
        database: field("DatabaseName"),
        username: field("Username"),
        windows_auth,
        password: String::new(),
        trust_certificate: true,
    };
    if c.server.trim().is_empty() || c.database.trim().is_empty() {
        bail!("Redgate project has an incomplete server/database");
    }
    Ok(Endpoint {
        connection: c,
        ..Endpoint::default()
    })
}
pub fn parse_redgate(xml: &str) -> Result<Project> {
    let doc = roxmltree::Document::parse(xml).context("Invalid Redgate project XML")?;
    let root = doc.root_element();
    if !root.has_tag_name("Project") {
        bail!("Expected a SQL Compare Project root");
    }
    let source = root
        .children()
        .find(|n| n.has_tag_name("DataSource1"))
        .context("Missing Redgate source")?;
    let target = root
        .children()
        .find(|n| n.has_tag_name("DataSource2"))
        .context("Missing Redgate target")?;
    let option_text = root
        .children()
        .find(|n| n.has_tag_name("Options"))
        .and_then(|n| n.text())
        .unwrap_or("");
    let enabled = |name: &str| option_text.split([',', ' ']).any(|s| s == name);
    Ok(Project {
        source: parse_source(source)?, target: parse_source(target)?,
        options: Options {
            ignore_comments: enabled("IgnoreComments"),
            ignore_module_settings: enabled("IgnoreQuotedIdentifiersAndAnsiNulls"),
            ignore_collation: enabled("IgnoreCollations"),
            ignore_column_order: !enabled("ForceColumnOrder"),
            auto_map_similar_columns: enabled("AutoMapSimilarColumns"),
            ignore_column_name_case: !enabled("UseCaseSensitiveObjectDefinition") && !enabled("CaseSensitiveObjectDefinition"),
            ignore_indexes: enabled("IgnoreIndexes"),
            ignore_foreign_keys: enabled("IgnoreForeignKeys"),
            ignore_check_constraints: enabled("IgnoreCheckConstraints") || enabled("IgnoreChecks"),
            ignore_triggers: enabled("IgnoreTriggers"),
            ignore_fill_factor: enabled("IgnoreFillFactor"),
            ..Options::default()
        }, redgate_path: None,
        import_notes: vec![
            "Redgate connections and supported Ignore options imported. Table/owner mappings and filters are not applied. Permissions, users/roles, extended properties and other unsupported object types can still produce different totals from Redgate.".into(),
            "Imported Redgate 14 connections use encrypted transport with Trust server certificate enabled. You can change this on the Data sources tab.".into()
        ]
    })
}
pub fn import_redgate(path: &Path, reuse_credentials: bool) -> Result<Project> {
    let bytes =
        std::fs::read(path).with_context(|| format!("Read Redgate project {}", path.display()))?;
    let xml = if bytes.starts_with(&[0xff, 0xfe]) {
        crate::catalog::decode_utf16(&bytes)?
    } else {
        String::from_utf8(bytes)?
            .trim_start_matches('\u{feff}')
            .to_owned()
    };
    let mut p = parse_redgate(&xml)?;
    p.redgate_path = Some(path.canonicalize()?);
    if reuse_credentials && (!p.source.connection.windows_auth || !p.target.connection.windows_auth)
    {
        match saved_credentials(path) {
            Ok(reply) => {
                p.source.connection.password = reply.password1;
                p.target.connection.password = reply.password2;
            }
            Err(_) => p.import_notes.push(
                "Saved passwords were unavailable. Enter SQL passwords on the Data sources tab."
                    .into(),
            ),
        }
    }
    Ok(p)
}
#[derive(Deserialize)]
struct PasswordReply {
    password1: String,
    password2: String,
}
fn saved_credentials(path: &Path) -> Result<PasswordReply> {
    use base64::Engine;
    let command = include_str!("redgate_credentials.ps1");
    let utf16: Vec<u8> = command.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-EncodedCommand",
        &encoded,
    ])
    .env("SC_REDGATE_PROJECT", path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let result = cmd
        .output()
        .context("Redgate credential bridge unavailable")?;
    if !result.status.success() {
        bail!("Saved credentials unavailable; enter SQL passwords manually");
    }
    // Output contains credentials: consume privately, never put it in error diagnostics.
    serde_json::from_slice(&result.stdout)
        .map_err(|_| anyhow::anyhow!("Invalid credential bridge response"))
}
pub fn redgate_project_directory() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Documents/SQL Compare/SharedProjects")
}
pub fn recent_redgate_projects() -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(redgate_project_directory())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("scp")))
        .collect();
    paths.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
    paths
}

#[cfg(windows)]
pub fn active_redgate_project() -> Result<PathBuf> {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::{
            EnumChildWindows, EnumWindows, GetWindowTextLengthW, GetWindowTextW,
            GetWindowThreadProcessId,
        },
    };
    struct Search {
        captions: Vec<String>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> BOOL {
        let len = GetWindowTextLengthW(hwnd);
        if len > 0 {
            let mut text = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32);
            let title = String::from_utf16_lossy(&text[..n.max(0) as usize]);
            if title.to_ascii_lowercase().contains(".scp") {
                (*(param as *mut Search)).captions.push(title);
            }
        }
        1
    }
    unsafe extern "system" fn top(hwnd: HWND, param: LPARAM) -> BOOL {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !process.is_null() {
            let mut buffer = vec![0u16; 32768];
            let mut size = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size);
            windows_sys::Win32::Foundation::CloseHandle(process);
            if ok != 0
                && String::from_utf16_lossy(&buffer[..size as usize])
                    .to_ascii_lowercase()
                    .ends_with("redgate.sqlcompare.ui.exe")
            {
                collect(hwnd, param);
                EnumChildWindows(hwnd, Some(collect), param);
            }
        }
        1
    }
    let mut search = Search { captions: vec![] };
    unsafe {
        EnumWindows(Some(top), &mut search as *mut Search as LPARAM);
    }
    let matches: Vec<_> = recent_redgate_projects()
        .into_iter()
        .filter(|p| {
            p.file_name().is_some_and(|f| {
                search.captions.iter().any(|t| {
                    t.to_lowercase()
                        .contains(&f.to_string_lossy().to_lowercase())
                })
            })
        })
        .collect();
    if matches.len() != 1 {
        bail!("Could not identify exactly one saved Redgate project from its open window. Open Edit project in Redgate, or use Import .scp to choose the file.");
    }
    Ok(matches[0].clone())
}
#[cfg(not(windows))]
pub fn active_redgate_project() -> Result<PathBuf> {
    bail!("Active Redgate project detection requires Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_projects_default_to_sql_auth_and_saved_windows_auth_is_preserved() {
        let mut project = Project::default();
        assert!(!project.source.connection.windows_auth);
        assert!(!project.target.connection.windows_auth);
        project.source.connection.windows_auth = true;
        let saved = serde_json::to_string(&project).unwrap();
        let loaded: Project = serde_json::from_str(&saved).unwrap();
        assert!(loaded.source.connection.windows_auth);
        assert!(!loaded.target.connection.windows_auth);
        // Command-line/integration connections retain their Windows-auth default.
        assert!(Connection::default().windows_auth);
    }
    const XML: &str = r#"<Project><DataSource1 type="LiveDatabaseSource"><ServerName>source,1433</ServerName><DatabaseName>App</DatabaseName><Username>dev</Username><AuthenticationType>SqlPassword</AuthenticationType><Password encrypted="1">never-store-this</Password></DataSource1><DataSource2 type="LiveDatabaseSource"><ServerName>target</ServerName><DatabaseName>Test</DatabaseName><AuthenticationType>Windows</AuthenticationType></DataSource2><Options>IgnoreComments,IgnorePermissions</Options></Project>"#;
    #[test]
    fn imports_direction_and_auth() {
        let p = parse_redgate(XML).unwrap();
        assert_eq!(p.source.connection.server, "source,1433");
        assert_eq!(p.target.connection.database, "Test");
        assert!(!p.source.connection.windows_auth);
        assert!(p.target.connection.windows_auth);
        assert!(p.options.ignore_comments);
        assert!(!p.options.allow_drops);
    }
    #[test]
    fn no_credentials_in_project_files() {
        let mut p = parse_redgate(XML).unwrap();
        p.source.connection.password = "private-password".into();
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("private-password"));
        assert!(!json.contains("never-store-this"));
        assert!(!json.contains("password"));
    }
    #[test]
    fn unsupported_sources_and_auth_rejected() {
        assert!(parse_redgate(&XML.replace("LiveDatabaseSource", "SnapshotSource")).is_err());
        assert!(parse_redgate(&XML.replace("SqlPassword", "AzureInteractive")).is_err());
    }
    #[test]
    fn xml_external_entities_rejected() {
        assert!(parse_redgate(
            "<!DOCTYPE Project [<!ENTITY x SYSTEM 'file:///private'>]><Project>&x;</Project>"
        )
        .is_err());
    }
}
