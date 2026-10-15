use crate::{
    model::{Kind, Object, Snapshot},
    sql::column_sql,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command};

#[derive(Clone, Serialize, Deserialize)]
pub struct Connection {
    pub server: String,
    pub database: String,
    pub windows_auth: bool,
    pub username: String,
    #[serde(skip)]
    pub password: String,
    pub trust_certificate: bool,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            server: "(localdb)\\MSSQLLocalDB".into(),
            database: "".into(),
            windows_auth: true,
            username: String::new(),
            password: String::new(),
            trust_certificate: false,
        }
    }
}
#[derive(Deserialize)]
struct RawObject {
    #[serde(flatten)]
    object: Object,
    #[serde(default)]
    dep_rows: Vec<Dep>,
}
#[derive(Deserialize)]
struct Dep {
    key: String,
}

pub fn capture(c: &Connection) -> Result<Snapshot> {
    capture_with_progress(c, |_| {})
}
pub fn capture_with_progress(c: &Connection, mut progress: impl FnMut(&str)) -> Result<Snapshot> {
    if c.server.trim().is_empty() || c.database.trim().is_empty() {
        bail!("Server and database are required");
    }
    let sql = include_str!("catalog.sql");
    progress("Connecting to SQL Server and reading schema catalogs");
    let raw = run_query(c, sql)?;
    progress("Decoding tables, columns, modules, indexes and constraints");
    let json = decode_hex_json(&raw)?;
    let rows: Vec<RawObject> = serde_json::from_str(&json).context("Decode schema catalog JSON")?;
    progress("Building object definitions and validating dependencies");
    snapshot_from_rows(c, rows)
}
/// Execute a reviewed planner script. SQLCMD failure stops subsequent batches.
pub fn execute_deployment(c: &Connection, sql: &str) -> Result<String> {
    if c.server.trim().is_empty() || c.database.trim().is_empty() {
        bail!("Deployment requires a live target server and database");
    }
    validate_deployment_script(sql)?;
    run_query_timeout(c, sql, 0)
        .context("Deployment execution failed; refresh the target before retrying")
}
pub fn validate_deployment_script(sql: &str) -> Result<()> {
    for line in sql.lines().map(str::trim) {
        if (line.starts_with(':') && !line.eq_ignore_ascii_case(":ON ERROR EXIT"))
            || line.starts_with("!!")
            || line.eq_ignore_ascii_case("ED")
        {
            bail!(
                "Unsupported SQLCMD directive in deployment script; only :ON ERROR EXIT is allowed"
            );
        }
        let mut parts = line.split_whitespace();
        if parts.next().is_some_and(|p| p.eq_ignore_ascii_case("GO"))
            && parts.next().is_some_and(|p| p.parse::<u32>().is_ok())
        {
            bail!("Repeated SQLCMD GO batches are not supported for deployment");
        }
    }
    Ok(())
}
fn decode_hex_json(raw: &str) -> Result<String> {
    let mut bytes = Vec::new();
    for line in raw.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if line.len() % 2 != 0 || !line.bytes().all(|c| c.is_ascii_hexdigit()) {
            bail!("Unexpected sqlcmd output: {line}");
        }
        for p in (0..line.len()).step_by(2) {
            bytes.push(u8::from_str_radix(&line[p..p + 2], 16)?);
        }
    }
    decode_utf16(&bytes)
}
fn snapshot_from_rows(c: &Connection, rows: Vec<RawObject>) -> Result<Snapshot> {
    let mut objects: Vec<Object> = rows
        .into_iter()
        .map(|r| {
            let mut o = r.object;
            o.dependencies = r.dep_rows.into_iter().map(|d| d.key).collect();
            o
        })
        .collect();
    let schemas: Vec<String> = objects
        .iter()
        .filter(|o| o.kind == Kind::Schema)
        .map(|o| o.schema.clone())
        .collect();
    let parents: std::collections::BTreeMap<_, _> = objects
        .iter()
        .filter(|o| matches!(o.kind, Kind::Table | Kind::View))
        .map(|o| ((o.schema.clone(), o.name.clone()), o.key()))
        .collect();
    for o in &mut objects {
        if o.kind == Kind::Table {
            o.definition = format!(
                "CREATE TABLE {} (\n    {}\n);",
                o.qualified(),
                o.columns
                    .iter()
                    .map(column_sql)
                    .collect::<Vec<_>>()
                    .join(",\n    ")
            );
        }
        if o.kind != Kind::Schema && schemas.contains(&o.schema) {
            o.dependencies
                .push(format!("Schema:{}::{}", o.schema, o.schema));
        }
        if let Some(p) = &o.parent {
            if let Some(key) = parents.get(&(o.schema.clone(), p.clone())) {
                o.dependencies.push(key.clone());
            }
        }
    }
    for o in &mut objects {
        o.dependencies.sort();
        o.dependencies.dedup();
    }
    objects.sort_by_key(Object::key);
    let s = Snapshot { format_version:1, label:format!("{} / {}",c.server,c.database), database:c.database.clone(), warnings:vec!["Scope: tables/columns/defaults, SQL modules, DML triggers, dbo-owned schemas, rowstore indexes, PK/UNIQUE/FK/CHECK constraints, synonyms. Permissions, users/roles, extended properties, sequences, CLR, full-text, physical filegroups and database options are not compared. Object names are case-sensitive. Cross-database and dynamic-SQL dependencies require manual review.".into()], objects };
    s.validate()?;
    Ok(s)
}
/// List online databases accessible to this login; no selected database is required.
pub fn list_databases(connection: &Connection) -> Result<Vec<String>> {
    if connection.server.trim().is_empty() {
        bail!("Enter a server name before fetching databases");
    }
    if !connection.windows_auth
        && (connection.username.trim().is_empty() || connection.password.is_empty())
    {
        bail!("Enter the SQL username and password before fetching databases");
    }
    let mut c = connection.clone();
    c.database = "master".into();
    let sql = r#"SET NOCOUNT ON;
DECLARE @json nvarchar(max)=(SELECT name FROM sys.databases WHERE state=0 AND HAS_DBACCESS(name)=1 ORDER BY name FOR JSON PATH);
DECLARE @pos int=1;
WHILE @pos<=LEN(@json) BEGIN
 PRINT CONVERT(varchar(max),CONVERT(varbinary(max),SUBSTRING(@json,@pos,1900)),2);
 SET @pos+=1900;
END;"#;
    let raw = run_query_timeout(&c, sql, 20)?;
    let json = decode_hex_json(&raw)?;
    #[derive(Deserialize)]
    struct Database {
        name: String,
    }
    let rows: Vec<Database> = serde_json::from_str(&json).context("Decode database list")?;
    let mut names: Vec<_> = rows.into_iter().map(|row| row.name).collect();
    names.sort();
    names.dedup();
    Ok(names)
}
pub fn decode_utf16(bytes: &[u8]) -> Result<String> {
    if !bytes.len().is_multiple_of(2) {
        bail!("Incomplete UTF-16 output");
    }
    let words: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    Ok(String::from_utf16(&words)?
        .trim_start_matches('\u{feff}')
        .to_owned())
}
fn sqlcmd_path() -> std::path::PathBuf {
    let p = Path::new(
        r"C:\Program Files\Microsoft SQL Server\Client SDK\ODBC\170\Tools\Binn\SQLCMD.EXE",
    );
    if p.exists() {
        p.to_owned()
    } else {
        "sqlcmd".into()
    }
}
fn run_query(c: &Connection, sql: &str) -> Result<String> {
    run_query_timeout(c, sql, 120)
}
fn run_query_timeout(c: &Connection, sql: &str, timeout: u32) -> Result<String> {
    let temp = tempfile::tempdir()?;
    let input = temp.path().join("catalog.sql");
    let output = temp.path().join("catalog.txt");
    let mut encoded = vec![0xff, 0xfe];
    for u in sql.encode_utf16() {
        encoded.extend(u.to_le_bytes());
    }
    std::fs::write(&input, encoded)?;
    let mut cmd = Command::new(sqlcmd_path());
    cmd.args([
        "-S",
        &c.server,
        "-d",
        &c.database,
        "-b",
        "-r",
        "1",
        "-l",
        "15",
        "-t",
        &timeout.to_string(),
        "-h",
        "-1",
        "-w",
        "65535",
        "-u",
        "-x",
    ]);
    // Do not use -X1: ODBC sqlcmd also disables SQLCMDPASSWORD, which would
    // force credentials into command-line arguments. Block startup scripts
    // explicitly; deployment directives are validated before launching sqlcmd.
    cmd.env_remove("SQLCMDINI");
    cmd.arg("-i").arg(&input).arg("-o").arg(&output);
    if !c.server.to_lowercase().starts_with("(localdb)\\") {
        cmd.arg("-N");
    }
    if c.windows_auth {
        cmd.arg("-E");
    } else {
        cmd.args(["-U", &c.username])
            .env("SQLCMDPASSWORD", &c.password);
    }
    if c.trust_certificate {
        cmd.arg("-C");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let result = cmd
        .output()
        .context("Cannot run sqlcmd. Install Microsoft SQL Server Command Line Utilities (ODBC)")?;
    let bytes = std::fs::read(&output).unwrap_or_default();
    let text =
        decode_utf16(&bytes).unwrap_or_else(|_| String::from_utf8_lossy(&bytes).into_owned());
    if !result.status.success() {
        bail!(
            "SQL Server command failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stderr),
            text
        );
    }
    Ok(text)
}
