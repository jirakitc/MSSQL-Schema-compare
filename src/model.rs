use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Schema,
    Table,
    View,
    Procedure,
    Function,
    Trigger,
    Index,
    ForeignKey,
    Check,
    PrimaryKey,
    Unique,
    Synonym,
}
impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub computed: Option<String>,
    #[serde(default)]
    pub persisted: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub default_name: Option<String>,
    #[serde(default)]
    pub collation: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object {
    pub schema: String,
    pub name: String,
    pub kind: Kind,
    pub definition: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default)]
    pub ansi_nulls: Option<bool>,
    #[serde(default)]
    pub quoted_identifier: Option<bool>,
}
impl Object {
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.kind,
            self.schema,
            self.parent.as_deref().unwrap_or(""),
            self.name
        )
    }
    pub fn qualified(&self) -> String {
        format!("{}.{}", quote(&self.schema), quote(&self.name))
    }
    pub fn parent_qualified(&self) -> String {
        format!(
            "{}.{}",
            quote(&self.schema),
            quote(self.parent.as_deref().unwrap_or(""))
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub format_version: u32,
    pub label: String,
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub objects: Vec<Object>,
}
impl Snapshot {
    pub fn validate(&self) -> Result<()> {
        if self.format_version != 1 {
            bail!("Unsupported snapshot format {}", self.format_version);
        }
        let mut keys = BTreeSet::new();
        for o in &self.objects {
            if o.schema.is_empty() || o.name.is_empty() || o.definition.trim().is_empty() {
                bail!("Incomplete object: {}", o.key());
            }
            if !keys.insert(o.key()) {
                bail!("Duplicate object: {}", o.key());
            }
            if o.kind == Kind::Table && o.columns.is_empty() {
                bail!("Table {} has no column metadata", o.qualified());
            }
            for c in &o.columns {
                if c.collation.as_ref().is_some_and(|s| {
                    s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                }) {
                    bail!("Invalid collation on {}.{}", o.qualified(), c.name);
                }
            }
            if matches!(
                o.kind,
                Kind::Index | Kind::ForeignKey | Kind::Check | Kind::PrimaryKey | Kind::Unique
            ) && o.parent.is_none()
            {
                bail!("Missing parent: {}", o.key());
            }
        }
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self> {
        let s: Self = serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("Read {}", path.display()))?,
        )?;
        s.validate()?;
        Ok(s)
    }
    pub fn write(&self, path: &Path) -> Result<()> {
        self.validate()?;
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
pub fn quote(s: &str) -> String {
    format!("[{}]", s.replace(']', "]]"))
}

/// UTF-8 BOM makes exported Unicode SQL unambiguous for Windows editors/sqlcmd.
pub fn write_script(path: &Path, sql: &str) -> Result<()> {
    std::fs::write(path, format!("\u{feff}{sql}"))?;
    Ok(())
}
