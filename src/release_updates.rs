use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;

pub const REPOSITORY_URL: &str = "https://github.com/jirakitc/MSSQL-Schema-compare";
pub const RELEASES_URL: &str = "https://github.com/jirakitc/MSSQL-Schema-compare/releases";
const API_URL: &str =
    "https://api.github.com/repos/jirakitc/MSSQL-Schema-compare/releases?per_page=100";

#[derive(Clone, Debug, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    pub published_at: Option<String>,
}
/// Include beta releases: GitHub's /latest endpoint excludes prereleases.
pub fn latest_from_json(body: &str) -> Result<Option<ReleaseInfo>> {
    let releases: Vec<ReleaseInfo> =
        serde_json::from_str(body).context("Invalid GitHub release response")?;
    Ok(releases
        .into_iter()
        .filter(|r| !r.draft && r.published_at.is_some() && !r.tag_name.trim().is_empty())
        .max_by(|a, b| a.published_at.cmp(&b.published_at)))
}
pub fn fetch_latest() -> Result<Option<ReleaseInfo>> {
    let curl = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .map(|root| root.join("System32/curl.exe"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| "curl.exe".into());
    let mut command = Command::new(curl);
    command.args([
        "--disable",
        "--silent",
        "--show-error",
        "--connect-timeout",
        "5",
        "--max-time",
        "15",
        "--max-filesize",
        "2097152",
        "--header",
        "Accept: application/vnd.github+json",
        "--user-agent",
        concat!("SchemaCompare/", env!("CARGO_PKG_VERSION")),
        "--write-out",
        "\n%{http_code}",
        API_URL,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command
        .output()
        .context("Cannot run the Windows HTTPS client")?;
    if !output.status.success() {
        bail!("Could not connect to GitHub. Check your internet connection and try again.");
    }
    let text = String::from_utf8(output.stdout).context("Invalid GitHub response encoding")?;
    let (body, status) = text
        .rsplit_once('\n')
        .context("Missing GitHub HTTP status")?;
    if status.trim() != "200" {
        bail!(
            "GitHub release check failed (HTTP {}). Try again later.",
            status.trim()
        );
    }
    latest_from_json(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn includes_beta_and_uses_publication_date_instead_of_api_order() {
        let body = r#"[{"tag_name":"v1.0.0","published_at":"2026-10-10T00:00:00Z"},{"tag_name":"v1.0.0-beta","prerelease":true,"published_at":"2026-10-15T00:00:00Z"}]"#;
        let latest = latest_from_json(body).unwrap().unwrap();
        assert_eq!(latest.tag_name, "v1.0.0-beta");
        assert!(latest.prerelease);
    }
    #[test]
    fn excludes_draft_unpublished_and_blank_tags() {
        let body = r#"[{"tag_name":"draft","draft":true,"published_at":"2026-10-16T00:00:00Z"},{"tag_name":"pending","published_at":null},{"tag_name":" ","published_at":"2026-10-16T00:00:00Z"},{"tag_name":"v1.0.0-beta","published_at":"2026-10-15T00:00:00Z"}]"#;
        assert_eq!(
            latest_from_json(body).unwrap().unwrap().tag_name,
            "v1.0.0-beta"
        );
    }
    #[test]
    fn empty_repo_has_no_release_and_invalid_responses_are_errors() {
        assert!(latest_from_json("[]").unwrap().is_none());
        assert!(latest_from_json(r#"{"message":"rate limited"}"#).is_err());
        assert!(latest_from_json("<html>bad gateway</html>").is_err());
    }
}
