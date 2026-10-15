use crate::{
    model::{quote, Column, Kind, Object, Snapshot},
    sql::{column_sql, module_ddl, normalized},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Status {
    SourceOnly,
    TargetOnly,
    Different,
    Equal,
}
impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::SourceOnly => "Source only",
                Self::TargetOnly => "Target only",
                Self::Different => "Different",
                Self::Equal => "Equal",
            }
        )
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Difference {
    pub key: String,
    pub status: Status,
    pub source: Option<Object>,
    pub target: Option<Object>,
    pub selected: bool,
    /// Catalog component keys represented by this logical comparison row.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
}
impl Difference {
    pub fn object(&self) -> &Object {
        self.source.as_ref().or(self.target.as_ref()).unwrap()
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub ignore_comments: bool,
    pub ignore_module_settings: bool,
    pub ignore_collation: bool,
    pub ignore_column_order: bool,
    pub ignore_column_name_case: bool,
    pub ignore_default_constraint_names: bool,
    pub ignore_indexes: bool,
    pub ignore_fill_factor: bool,
    pub ignore_foreign_keys: bool,
    pub ignore_check_constraints: bool,
    pub ignore_triggers: bool,
    pub allow_drops: bool,
}
impl Options {
    pub fn ignores_kind(&self, kind: &Kind) -> bool {
        match kind {
            Kind::Index => self.ignore_indexes,
            Kind::ForeignKey => self.ignore_foreign_keys,
            Kind::Check => self.ignore_check_constraints,
            Kind::Trigger => self.ignore_triggers,
            _ => false,
        }
    }
    pub fn ignore_labels(&self) -> Vec<&'static str> {
        [
            (self.ignore_comments, "SQL comments"),
            (self.ignore_module_settings, "module SET options"),
            (self.ignore_collation, "column collation"),
            (self.ignore_column_order, "column order"),
            (self.ignore_column_name_case, "column name case"),
            (
                self.ignore_default_constraint_names,
                "default constraint names",
            ),
            (self.ignore_indexes, "indexes"),
            (self.ignore_fill_factor, "index fill factor and padding"),
            (self.ignore_foreign_keys, "foreign keys"),
            (self.ignore_check_constraints, "CHECK constraints"),
            (self.ignore_triggers, "DML triggers"),
        ]
        .into_iter()
        .filter_map(|(enabled, label)| enabled.then_some(label))
        .collect()
    }
}

fn column_name(name: &str, options: &Options) -> String {
    if options.ignore_column_name_case {
        name.to_lowercase()
    } else {
        name.to_owned()
    }
}
fn column_equal(a: &Column, b: &Column, options: &Options) -> bool {
    let expression_equal = |a: &Option<String>, b: &Option<String>| match (a, b) {
        (Some(a), Some(b)) => {
            normalized(a, options.ignore_comments) == normalized(b, options.ignore_comments)
        }
        (None, None) => true,
        _ => false,
    };
    column_name(&a.name, options) == column_name(&b.name, options)
        && a.data_type == b.data_type
        && a.nullable == b.nullable
        && a.identity == b.identity
        && expression_equal(&a.computed, &b.computed)
        && a.persisted == b.persisted
        && expression_equal(&a.default, &b.default)
        && (options.ignore_default_constraint_names || a.default_name == b.default_name)
        && (options.ignore_collation || a.collation == b.collation)
}
fn columns_equal(a: &[Column], b: &[Column], options: &Options) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if options.ignore_column_order {
        let columns: BTreeMap<_, _> = b
            .iter()
            .map(|c| (column_name(&c.name, options), c))
            .collect();
        a.iter().all(|c| {
            columns
                .get(&column_name(&c.name, options))
                .is_some_and(|other| column_equal(c, other, options))
        })
    } else {
        a.iter().zip(b).all(|(a, b)| column_equal(a, b, options))
    }
}
fn object_equal(a: &Object, b: &Object, options: &Options) -> bool {
    let definition_equal = if a.kind == Kind::Table {
        columns_equal(&a.columns, &b.columns, options)
    } else {
        let normalize = |o: &Object| {
            let mut tokens = normalized(&o.definition, options.ignore_comments);
            if is_module(&o.kind) {
                if let Some(start) = tokens.iter().position(|t| {
                    t.eq_ignore_ascii_case("CREATE") || t.eq_ignore_ascii_case("ALTER")
                }) {
                    let mut kind = start + 1;
                    if tokens
                        .get(kind)
                        .is_some_and(|t| t.eq_ignore_ascii_case("OR"))
                    {
                        kind += 2;
                    }
                    let mut end = kind + 2;
                    while tokens.get(end).is_some_and(|t| t == ".") {
                        end += 2;
                    }
                    if end <= tokens.len() {
                        let module_kind = match o.kind {
                            Kind::Procedure => "PROCEDURE",
                            Kind::Function => "FUNCTION",
                            Kind::View => "VIEW",
                            _ => "TRIGGER",
                        };
                        tokens.splice(
                            start..end,
                            ["CREATE".into(), module_kind.into(), o.qualified()],
                        );
                    }
                }
            }
            if matches!(o.kind, Kind::Index | Kind::PrimaryKey | Kind::Unique) {
                if let Some(start) = tokens.iter().position(|t| t == "INCLUDE") {
                    if tokens.get(start + 1).is_some_and(|t| t == "(") {
                        if let Some(end) = (start + 2..tokens.len()).find(|i| tokens[*i] == ")") {
                            let mut names: Vec<_> = tokens[start + 2..end]
                                .iter()
                                .filter(|t| t.as_str() != ",")
                                .cloned()
                                .collect();
                            names.sort();
                            tokens.splice(start + 2..end, names);
                        }
                    }
                }
            }
            if options.ignore_fill_factor
                && matches!(o.kind, Kind::Index | Kind::PrimaryKey | Kind::Unique)
            {
                // Capture emits named WITH options. Remove only these complete assignments,
                // keeping every other index property and identifier intact.
                let mut i = 0;
                while i + 2 < tokens.len() {
                    if matches!(tokens[i].as_str(), "PAD_INDEX" | "FILLFACTOR")
                        && tokens[i + 1] == "="
                    {
                        let end = i + 3;
                        if tokens.get(end).is_some_and(|t| t == ",") {
                            tokens.drain(i..end + 1);
                        } else if i > 0 && tokens[i - 1] == "," {
                            tokens.drain(i - 1..end);
                            i -= 1;
                        } else {
                            tokens.drain(i..end);
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            tokens
        };
        normalize(a) == normalize(b)
    };
    definition_equal
        && (options.ignore_module_settings
            || (a.ansi_nulls == b.ansi_nulls && a.quoted_identifier == b.quoted_identifier))
}
#[derive(Default, Serialize)]
pub struct Plan {
    pub sql: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub actions: usize,
}
impl Plan {
    pub fn ready(&self) -> bool {
        self.blockers.is_empty() && self.actions > 0
    }
}

pub fn compare(source: &Snapshot, target: &Snapshot, options: &Options) -> Vec<Difference> {
    let s: BTreeMap<_, _> = source
        .objects
        .iter()
        .filter(|o| !options.ignores_kind(&o.kind))
        .map(|o| (o.key(), o))
        .collect();
    let t: BTreeMap<_, _> = target
        .objects
        .iter()
        .filter(|o| !options.ignores_kind(&o.kind))
        .map(|o| (o.key(), o))
        .collect();
    let keys: BTreeSet<_> = s.keys().chain(t.keys()).cloned().collect();
    keys.into_iter()
        .map(|key| {
            let a = s.get(&key).copied();
            let b = t.get(&key).copied();
            let status = match (a, b) {
                (Some(a), Some(b)) if object_equal(a, b, options) => Status::Equal,
                (Some(_), Some(_)) => Status::Different,
                (Some(_), None) => Status::SourceOnly,
                _ => Status::TargetOnly,
            };
            let selected = matches!(status, Status::SourceOnly | Status::Different);
            Difference {
                key,
                status,
                source: a.cloned(),
                target: b.cloned(),
                selected,
                components: vec![],
            }
        })
        .collect()
}
fn is_module(k: &Kind) -> bool {
    matches!(
        k,
        Kind::View | Kind::Procedure | Kind::Function | Kind::Trigger
    )
}
fn drop_sql(o: &Object) -> String {
    match o.kind {
        Kind::Index => format!("DROP INDEX {} ON {};", quote(&o.name), o.parent_qualified()),
        Kind::ForeignKey | Kind::Check | Kind::PrimaryKey | Kind::Unique => format!(
            "ALTER TABLE {} DROP CONSTRAINT {};",
            o.parent_qualified(),
            quote(&o.name)
        ),
        Kind::Schema => format!("DROP SCHEMA {};", quote(&o.name)),
        _ => format!(
            "DROP {} {};",
            match o.kind {
                Kind::Procedure => "PROCEDURE",
                Kind::Function => "FUNCTION",
                Kind::Trigger => "TRIGGER",
                Kind::Table => "TABLE",
                Kind::View => "VIEW",
                Kind::Synonym => "SYNONYM",
                _ => unreachable!(),
            },
            o.qualified()
        ),
    }
}
struct TableMigration {
    sql: String,
    alters_columns: bool,
}

fn removal_affects(object: &Object, removed: &BTreeMap<String, Vec<String>>) -> bool {
    let parent_key = format!(
        "Table:{}::{}",
        object.schema,
        object.parent.as_deref().unwrap_or("")
    );
    let tokens = crate::sql::tokens(&object.definition, true);
    removed.iter().any(|(key, names)| {
        (parent_key == *key || object.dependencies.contains(key))
            && names.iter().any(|name| {
                tokens
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(name) || t.eq_ignore_ascii_case(&quote(name)))
            })
    })
}

fn column_guards(table: &Object, column: &Column) -> String {
    let table_literal = table.qualified().replace('\'', "''");
    let column_literal = column.name.replace('\'', "''");
    format!("IF EXISTS (SELECT 1 FROM sys.stats st JOIN sys.stats_columns sc ON sc.object_id=st.object_id AND sc.stats_id=st.stats_id JOIN sys.columns c ON c.object_id=sc.object_id AND c.column_id=sc.column_id WHERE st.object_id=OBJECT_ID(N'{table_literal}') AND c.name=N'{column_literal}' AND st.user_created=1 AND NOT EXISTS (SELECT 1 FROM sys.indexes i WHERE i.object_id=st.object_id AND i.index_id=st.stats_id)) THROW 51020, 'Column migration requires review of user-created statistics on this column.', 1;\nIF EXISTS (SELECT 1 FROM sys.sql_expression_dependencies WHERE referenced_id=OBJECT_ID(N'{table_literal}') AND is_schema_bound_reference=1) THROW 51021, 'Column migration requires review of schema-bound views/functions.', 1;\n")
}

/// Accept only conversions that can represent every value of the old type exactly.
fn widening_type(old: &str, new: &str) -> bool {
    fn parse(s: &str) -> (String, Vec<String>) {
        let s = s.replace(['[', ']', ' '], "").to_ascii_lowercase();
        let (name, args) = s.split_once('(').unwrap_or((&s, ""));
        (
            name.into(),
            args.trim_end_matches(')')
                .split(',')
                .filter(|p| !p.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }
    fn integer(name: &str) -> Option<(u8, u8)> {
        match name {
            "tinyint" => Some((0, 3)),
            "smallint" => Some((1, 5)),
            "int" => Some((2, 10)),
            "bigint" => Some((3, 19)),
            _ => None,
        }
    }
    fn decimal(name: &str, args: &[String]) -> Option<(u8, u8)> {
        if !matches!(name, "decimal" | "numeric") || args.len() != 2 {
            return None;
        }
        let p = args[0].parse::<u8>().ok()?;
        let s = args[1].parse::<u8>().ok()?;
        if p == 0 || p > 38 || s > p {
            return None;
        }
        Some((p - s, s))
    }
    let (a, aa) = parse(old);
    let (b, ba) = parse(new);
    if let (Some((old_rank, _)), Some((new_rank, _))) = (integer(&a), integer(&b)) {
        return new_rank >= old_rank;
    }
    if let (Some((_, digits)), Some((integral, _))) = (integer(&a), decimal(&b, &ba)) {
        return integral >= digits;
    }
    if let (Some((old_i, old_s)), Some((new_i, new_s))) = (decimal(&a, &aa), decimal(&b, &ba)) {
        return new_i >= old_i && new_s >= old_s;
    }
    if a == b
        && matches!(a.as_str(), "varchar" | "nvarchar" | "varbinary")
        && aa.len() == 1
        && ba.len() == 1
    {
        return ba[0] == "max"
            || (aa[0] != "max"
                && aa[0]
                    .parse::<u32>()
                    .ok()
                    .zip(ba[0].parse::<u32>().ok())
                    .is_some_and(|(old, new)| new >= old));
    }
    false
}

fn table_migration(
    source: &Object,
    target: &Object,
    options: &Options,
) -> Result<TableMigration, String> {
    let target_names: BTreeSet<_> = target
        .columns
        .iter()
        .map(|c| column_name(&c.name, options))
        .collect();
    let existing: Vec<_> = source
        .columns
        .iter()
        .filter(|c| target_names.contains(&column_name(&c.name, options)))
        .cloned()
        .collect();
    let retained: Vec<_> = target
        .columns
        .iter()
        .filter(|old| {
            existing
                .iter()
                .any(|c| column_name(&c.name, options) == column_name(&old.name, options))
        })
        .collect();
    if !options.ignore_column_order
        && (!source
            .columns
            .iter()
            .take(retained.len())
            .zip(&retained)
            .all(|(a, b)| column_name(&a.name, options) == column_name(&b.name, options)))
    {
        return Err("Column reordering requires a reviewed manual migration (or enable Ignore column order)".into());
    }
    let mut sql = String::new();
    let mut alters_columns = false;
    for old in &target.columns {
        let desired = existing
            .iter()
            .find(|c| column_name(&c.name, options) == column_name(&old.name, options));
        let Some(desired) = desired else {
            if target.columns.iter().any(|c| {
                c.computed.is_some()
                    && existing
                        .iter()
                        .any(|e| column_name(&e.name, options) == column_name(&c.name, options))
            }) {
                return Err(format!(
                    "Column {}: review retained computed-column dependencies before DROP COLUMN",
                    quote(&old.name)
                ));
            }
            alters_columns = true;
            sql.push_str(&column_guards(source, old));
            if let Some(name) = &old.default_name {
                sql.push_str(&format!(
                    "ALTER TABLE {} DROP CONSTRAINT {};\n",
                    source.qualified(),
                    quote(name)
                ));
            }
            sql.push_str(&format!(
                "ALTER TABLE {} DROP COLUMN {};\n",
                source.qualified(),
                quote(&old.name)
            ));
            continue;
        };
        if column_equal(desired, old, options) {
            continue;
        }
        let mut expected = old.clone();
        expected.data_type = desired.data_type.clone();
        expected.nullable = desired.nullable;
        if !column_equal(desired, &expected, options) || old.computed.is_some() {
            return Err(format!("Column {}: default, identity, computed or collation changes require a reviewed manual migration",quote(&old.name)));
        }
        if old.nullable && !desired.nullable {
            sql.push_str(&format!("IF EXISTS(SELECT 1 FROM {} WITH (TABLOCKX,HOLDLOCK) WHERE {} IS NULL) THROW 51022, 'Column {} contains NULL values; cannot apply Source NOT NULL.', 1;\n",source.qualified(),quote(&old.name),old.name.replace('\'',"''")));
        }
        if old.identity.is_some()
            && old.data_type != desired.data_type
            && !matches!(
                desired
                    .data_type
                    .replace(['[', ']'], "")
                    .to_ascii_lowercase()
                    .as_str(),
                "tinyint" | "smallint" | "int" | "bigint"
            )
        {
            return Err(format!(
                "Column {}: this IDENTITY datatype requires a reviewed manual migration",
                quote(&old.name)
            ));
        }
        if old.data_type != desired.data_type && !widening_type(&old.data_type, &desired.data_type)
        {
            // Round-trip comparison detects overflow, truncation and rounding.
            // TRY_CONVERT alone is insufficient because narrowing strings truncates.
            let col = quote(&old.name);
            sql.push_str(&format!("IF EXISTS(SELECT 1 FROM {} WITH (TABLOCKX,HOLDLOCK) WHERE {col} IS NOT NULL AND (TRY_CONVERT({}, {col}) IS NULL OR TRY_CONVERT({}, TRY_CONVERT({}, {col})) IS NULL OR CONVERT(varbinary(max), TRY_CONVERT({}, TRY_CONVERT({}, {col}))) <> CONVERT(varbinary(max),{col}))) THROW 51023, 'Column {} has values that cannot fit the Source datatype without truncation, rounding or conversion loss.', 1;\n",source.qualified(),desired.data_type,old.data_type,desired.data_type,old.data_type,desired.data_type,old.name.replace('\'',"''")));
        }
        if target.columns.iter().any(|c| c.computed.is_some()) {
            return Err(format!("Column {}: this table has computed columns; review their dependencies before ALTER COLUMN",quote(&old.name)));
        }
        alters_columns = true;
        // Catalogs do not capture standalone statistics. Keep them intact and fail
        // before changing anything instead of silently dropping user statistics.
        sql.push_str(&column_guards(source, old));
        if old.default.is_some() != old.default_name.is_some() {
            return Err(format!("Column {}: missing default constraint name; refresh the catalog before ALTER COLUMN",quote(&old.name)));
        }
        if let Some(name) = &old.default_name {
            sql.push_str(&format!(
                "ALTER TABLE {} DROP CONSTRAINT {};\n",
                source.qualified(),
                quote(name)
            ));
        }
        let mut column = desired.clone();
        column.name = old.name.clone();
        column.default = None;
        column.default_name = None;
        column.identity = None;
        if options.ignore_collation {
            column.collation = old.collation.clone();
        }
        sql.push_str(&format!(
            "ALTER TABLE {} ALTER COLUMN {};\n",
            source.qualified(),
            column_sql(&column)
        ));
        if let Some(default) = &old.default {
            sql.push_str(&format!(
                "ALTER TABLE {} ADD CONSTRAINT {} DEFAULT {} FOR {};\n",
                source.qualified(),
                quote(old.default_name.as_ref().unwrap()),
                default,
                quote(&old.name)
            ));
        }
    }
    let new: Vec<_> = source
        .columns
        .iter()
        .filter(|c| !target_names.contains(&column_name(&c.name, options)))
        .collect();
    if new
        .iter()
        .any(|c| c.identity.is_some() || c.computed.is_some())
    {
        return Err("Adding identity/computed columns requires a reviewed manual migration".into());
    }
    if !new.is_empty() {
        let mut addition = String::new();
        for c in &new {
            if !c.nullable && c.default.is_none() {
                addition.push_str(&format!("IF EXISTS(SELECT 1 FROM {} WITH (TABLOCKX,HOLDLOCK)) THROW 51024, 'New Source column {} is NOT NULL without a default; existing rows require a backfill.', 1;\n",source.qualified(),c.name.replace('\'',"''")));
            }
        }
        addition.push_str(&format!(
            "ALTER TABLE {} ADD\n    {};",
            source.qualified(),
            new.iter()
                .map(|c| column_sql(c))
                .collect::<Vec<_>>()
                .join(",\n    ")
        ));
        // Add first: SQL Server cannot drop the last remaining column of a table.
        sql = format!("{addition}\n{sql}");
    }
    Ok(TableMigration {
        sql,
        alters_columns,
    })
}
fn order(objects: &BTreeMap<String, (&Object, String)>) -> Result<Vec<String>, String> {
    let mut done = BTreeSet::new();
    let mut result = Vec::new();
    while done.len() < objects.len() {
        let next = objects.iter().find(|(k, (o, _))| {
            !done.contains(*k)
                && o.dependencies
                    .iter()
                    .all(|d| d == *k || !objects.contains_key(d) || done.contains(d))
        });
        if let Some((k, _)) = next {
            done.insert(k.clone());
            result.push(k.clone());
        } else {
            return Err(format!(
                "Dependency cycle: {}",
                objects
                    .keys()
                    .filter(|k| !done.contains(*k))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    Ok(result)
}
pub fn plan(source: &Snapshot, target: &Snapshot, diffs: &[Difference], options: &Options) -> Plan {
    if diffs.iter().any(|d| !d.components.is_empty()) {
        let selected: BTreeSet<_> = diffs
            .iter()
            .filter(|d| d.selected)
            .flat_map(|d| d.components.iter().cloned())
            .collect();
        let mut components = compare(source, target, options);
        for d in &mut components {
            d.selected = selected.contains(&d.key) && d.status != Status::Equal;
        }
        return plan(source, target, &components, options);
    }
    let mut p = Plan {
        warnings: source
            .warnings
            .iter()
            .chain(&target.warnings)
            .cloned()
            .collect(),
        ..Plan::default()
    };
    p.warnings.sort();
    p.warnings.dedup();
    if !options.ignore_labels().is_empty() {
        p.warnings.push(format!("Ignore options active: {}. Ignored object types are excluded from comparison/deployment; dependency checks still include retained target objects.",options.ignore_labels().join(", ")));
    }
    p.warnings.push("Review against the same target snapshot before execution. Dependencies from dynamic SQL or other databases are not resolved. Run with sqlcmd -b or SSMS SQLCMD mode; ON ERROR EXIT stops later batches after an error.".into());
    let target_keys: BTreeSet<_> = target.objects.iter().map(Object::key).collect();
    let chosen: BTreeSet<_> = diffs
        .iter()
        .filter(|d| {
            d.selected && d.status != Status::Equal && !options.ignores_kind(&d.object().kind)
        })
        .map(|d| d.key.clone())
        .collect();
    let changed: BTreeSet<_> = diffs
        .iter()
        .filter(|d| d.status == Status::Different)
        .map(|d| d.key.clone())
        .collect();
    let deleting: BTreeSet<_> = diffs
        .iter()
        .filter(|d| chosen.contains(&d.key) && d.status == Status::TargetOnly)
        .map(|d| d.key.clone())
        .collect();
    let removed_columns: BTreeMap<_, _> = diffs
        .iter()
        .filter(|d| chosen.contains(&d.key) && d.object().kind == Kind::Table)
        .filter_map(|d| {
            d.source.as_ref().zip(d.target.as_ref()).map(|(s, t)| {
                (
                    d.key.clone(),
                    t.columns
                        .iter()
                        .filter(|old| {
                            !s.columns.iter().any(|c| {
                                column_name(&c.name, options) == column_name(&old.name, options)
                            })
                        })
                        .map(|c| c.name.clone())
                        .collect::<Vec<_>>(),
                )
            })
        })
        .filter(|(_, names)| !names.is_empty())
        .collect();
    let mut creates = BTreeMap::new();
    let mut drops = BTreeMap::new();
    let mut altered_tables = BTreeSet::new();
    for d in diffs.iter().filter(|d| chosen.contains(&d.key)) {
        let o = d.object();
        if let Some(s) = &d.source {
            for dep in &s.dependencies {
                if dep != &d.key
                    && (deleting.contains(dep)
                        || ((!target_keys.contains(dep) || changed.contains(dep))
                            && !chosen.contains(dep)))
                {
                    p.blockers.push(format!(
                        "{} needs {}. Select its dependency or retain it in the target.",
                        o.qualified(),
                        dep
                    ));
                }
            }
        }
        match d.status {
            Status::SourceOnly => {
                let sql = if is_module(&o.kind) {
                    module_ddl(&o.definition, false)
                } else {
                    Ok(o.definition.clone())
                };
                match sql {
                    Ok(sql) => {
                        creates.insert(d.key.clone(), (o, sql));
                    }
                    Err(e) => p.blockers.push(format!("{}: {e}", o.qualified())),
                }
            }
            Status::Different => {
                if o.kind == Kind::Table {
                    match table_migration(o, d.target.as_ref().unwrap(), options) {
                        Ok(migration) => {
                            if migration.alters_columns {
                                altered_tables.insert(d.key.clone());
                                p.warnings.push(format!("{}: apply Source column definitions; related indexes/constraints are rebuilt in the transaction. Datatype narrowing checks existing values first. This may lock the table and take time.",o.qualified()));
                                if let Some(names) = removed_columns.get(&d.key) {
                                    p.warnings.push(format!("{}: DROP COLUMN {} to match Source. Data in these removed columns is deleted. Renamed columns are treated as drop/add; values are not copied automatically.",o.qualified(),names.iter().map(|n|quote(n)).collect::<Vec<_>>().join(", ")));
                                }
                            }
                            creates.insert(d.key.clone(), (o, migration.sql));
                        }
                        Err(e) => p.blockers.push(format!("{}: {e}", o.qualified())),
                    }
                } else if is_module(&o.kind) {
                    match module_ddl(&o.definition, true) {
                        Ok(sql) => {
                            creates.insert(d.key.clone(), (o, sql));
                        }
                        Err(e) => p.blockers.push(format!("{}: {e}", o.qualified())),
                    }
                } else if o.kind == Kind::Schema {
                    p.blockers.push(format!(
                        "{}: schema changes require manual review",
                        o.qualified()
                    ));
                } else {
                    drops.insert(
                        d.key.clone(),
                        (
                            d.target.as_ref().unwrap(),
                            drop_sql(d.target.as_ref().unwrap()),
                        ),
                    );
                    creates.insert(d.key.clone(), (o, o.definition.clone()));
                    p.warnings.push(format!("{} will be dropped and recreated; index builds and constraint validation can lock large tables.",o.qualified()));
                }
            }
            Status::TargetOnly => {
                if !options.allow_drops && !removal_affects(o, &removed_columns) {
                    p.blockers.push(format!(
                        "{}: enable destructive drops to remove target-only objects",
                        o.qualified()
                    ));
                } else {
                    drops.insert(d.key.clone(), (o, drop_sql(o)));
                    p.warnings.push(format!(
                        "DROP {} {}: data or dependent behavior may be lost",
                        o.kind,
                        o.qualified()
                    ));
                }
            }
            Status::Equal => {}
        }
    }
    // Recreate even identical/ignored target dependencies. Ignore options must
    // never cause an existing FK/index/check to disappear during a column ALTER.
    for dependency in &target.objects {
        if is_module(&dependency.kind)
            && normalized(&dependency.definition, true)
                .iter()
                .any(|t| t == "SCHEMABINDING")
            && dependency
                .dependencies
                .iter()
                .any(|key| altered_tables.contains(key))
        {
            p.blockers.push(format!("{}: schema-bound dependency on an altered table requires a reviewed manual migration",dependency.qualified()));
        }
        if !matches!(
            dependency.kind,
            Kind::Index
                | Kind::PrimaryKey
                | Kind::Unique
                | Kind::Check
                | Kind::ForeignKey
                | Kind::Trigger
        ) {
            continue;
        }
        let removed_dependency = removal_affects(dependency, &removed_columns);
        if dependency.kind == Kind::Trigger && !removed_dependency {
            continue;
        }
        let parent_key = format!(
            "Table:{}::{}",
            dependency.schema,
            dependency.parent.as_deref().unwrap_or("")
        );
        if !altered_tables.contains(&parent_key)
            && !(dependency.kind == Kind::ForeignKey
                && dependency
                    .dependencies
                    .iter()
                    .any(|key| altered_tables.contains(key)))
        {
            continue;
        }
        let key = dependency.key();
        drops
            .entry(key.clone())
            .or_insert_with(|| (dependency, drop_sql(dependency)));
        if removed_dependency {
            if let Some(desired) = source.objects.iter().find(|o| o.key() == key) {
                creates.insert(key, (desired, desired.definition.clone()));
            } else {
                creates.remove(&key);
            }
        } else if !deleting.contains(&key) {
            creates
                .entry(key)
                .or_insert_with(|| (dependency, dependency.definition.clone()));
        }
    }
    // Source dependencies introduced by a drop/add rename may be excluded by
    // Ignore options. They still must accompany the new column definition.
    for dependency in &source.objects {
        if !matches!(
            dependency.kind,
            Kind::Index
                | Kind::PrimaryKey
                | Kind::Unique
                | Kind::Check
                | Kind::ForeignKey
                | Kind::Trigger
        ) {
            continue;
        }
        let parent_key = format!(
            "Table:{}::{}",
            dependency.schema,
            dependency.parent.as_deref().unwrap_or("")
        );
        if removed_columns.contains_key(&parent_key) && !target_keys.contains(&dependency.key()) {
            creates
                .entry(dependency.key())
                .or_insert_with(|| (dependency, dependency.definition.clone()));
        }
    }
    for (object, _) in creates.values() {
        for key in &object.dependencies {
            if !target_keys.contains(key) && !creates.contains_key(key) {
                p.blockers.push(format!(
                    "{}: missing dependency {key} required by Source column migration",
                    object.qualified()
                ));
            }
        }
    }
    for (key, (o, _)) in &drops {
        for dependent in &target.objects {
            if dependent.key() != *key
                && dependent.dependencies.contains(key)
                && !drops.contains_key(&dependent.key())
            {
                p.blockers.push(format!(
                    "Cannot drop {} while {} depends on it. Remove/recreate dependents explicitly.",
                    o.qualified(),
                    dependent.qualified()
                ));
            }
        }
    }
    let create_order = order(&creates);
    let drop_order = order(&drops);
    if let Err(e) = &create_order {
        p.blockers.push(e.clone());
    }
    if let Err(e) = &drop_order {
        p.blockers.push(e.clone());
    }
    p.actions = creates.len() + drops.len();
    if !p.blockers.is_empty() {
        p.sql = format!(
            "-- NO EXECUTABLE SCRIPT: resolve these blockers first.\n{}",
            p.blockers
                .iter()
                .map(|b| format!("-- {}", b.replace(['\r', '\n'], " ")))
                .collect::<Vec<_>>()
                .join("\n")
        );
        return p;
    }
    let mut sql=String::from("-- Schema Compare | source -> target | REVIEW BEFORE EXECUTION\n-- Requires sqlcmd -b or SSMS with SQLCMD mode enabled.\n:ON ERROR EXIT\n");
    if target.database.is_empty() {
        p.blockers.push(
            "Target snapshot has no database name. Capture the intended target first.".into(),
        );
        p.sql = "-- Script blocked: target database name is required.".into();
        return p;
    }
    sql.push_str(&format!("USE {};\nGO\nSET XACT_ABORT ON;\nSET ANSI_NULLS ON;\nSET QUOTED_IDENTIFIER ON;\nSET ANSI_PADDING ON;\nSET ANSI_WARNINGS ON;\nSET CONCAT_NULL_YIELDS_NULL ON;\nSET ARITHABORT ON;\nSET NUMERIC_ROUNDABORT OFF;\nBEGIN TRANSACTION;\nGO\n",quote(&target.database)));
    for k in drop_order.unwrap().into_iter().rev() {
        sql.push_str(&drops[&k].1);
        sql.push_str("\nGO\n");
    }
    for k in create_order.unwrap() {
        let (o, ddl) = &creates[&k];
        if is_module(&o.kind) {
            let settings = if options.ignore_module_settings {
                target.objects.iter().find(|t| t.key() == k).unwrap_or(o)
            } else {
                o
            };
            sql.push_str(&format!(
                "SET ANSI_NULLS {};\nSET QUOTED_IDENTIFIER {};\nGO\n",
                if settings.ansi_nulls.unwrap_or(true) {
                    "ON"
                } else {
                    "OFF"
                },
                if settings.quoted_identifier.unwrap_or(true) {
                    "ON"
                } else {
                    "OFF"
                }
            ));
        } else {
            sql.push_str("SET ANSI_NULLS ON;\nSET QUOTED_IDENTIFIER ON;\nGO\n");
        }
        sql.push_str(ddl);
        sql.push_str("\nGO\n");
    }
    sql.push_str("COMMIT TRANSACTION;\nGO\n");
    p.sql = sql;
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snap(objects: Vec<Object>) -> Snapshot {
        Snapshot {
            format_version: 1,
            label: "test".into(),
            database: "Target".into(),
            warnings: vec![],
            objects,
        }
    }
    fn view(name: &str, body: &str) -> Object {
        Object {
            schema: "dbo".into(),
            name: name.into(),
            kind: Kind::View,
            definition: format!("CREATE VIEW [dbo].[{name}] AS {body}"),
            parent: None,
            dependencies: vec![],
            columns: vec![],
            ansi_nulls: Some(true),
            quoted_identifier: Some(true),
        }
    }
    #[test]
    fn four_statuses() {
        let a = snap(vec![
            view("a", "SELECT 1"),
            view("b", "SELECT 2"),
            view("c", "SELECT 1"),
        ]);
        let b = snap(vec![
            view("b", "SELECT 3"),
            view("c", "SELECT 1"),
            view("d", "SELECT 4"),
        ]);
        let d = compare(&a, &b, &Options::default());
        assert_eq!(
            d.iter().map(|d| d.status.clone()).collect::<Vec<_>>(),
            vec![
                Status::SourceOnly,
                Status::Different,
                Status::Equal,
                Status::TargetOnly
            ]
        );
        assert!(!d[3].selected);
    }
    #[test]
    fn dependencies_ordered_and_unselected_blocked() {
        let base = view("z", "SELECT 1 AS id");
        let mut dependent = view("a", "SELECT * FROM dbo.z");
        dependent.dependencies.push(base.key());
        let a = snap(vec![dependent, base]);
        let b = snap(vec![]);
        let mut d = compare(&a, &b, &Options::default());
        let p = plan(&a, &b, &d, &Options::default());
        assert!(p.ready());
        assert!(
            p.sql.find("CREATE VIEW [dbo].[z]").unwrap()
                < p.sql.find("CREATE VIEW [dbo].[a]").unwrap()
        );
        d[1].selected = false;
        assert!(!plan(&a, &b, &d, &Options::default()).ready());
    }
    #[test]
    fn cycles_blocked() {
        let mut a = view("a", "SELECT * FROM b");
        let mut b = view("b", "SELECT * FROM a");
        a.dependencies.push(b.key());
        b.dependencies.push(a.key());
        let s = snap(vec![a, b]);
        let t = snap(vec![]);
        assert!(!plan(
            &s,
            &t,
            &compare(&s, &t, &Options::default()),
            &Options::default()
        )
        .ready());
    }
    #[test]
    fn drops_require_opt_in() {
        let s = snap(vec![]);
        let t = snap(vec![view("a", "SELECT 1")]);
        let mut d = compare(&s, &t, &Options::default());
        d[0].selected = true;
        assert!(!plan(&s, &t, &d, &Options::default()).ready());
        assert!(plan(
            &s,
            &t,
            &d,
            &Options {
                allow_drops: true,
                ..Options::default()
            }
        )
        .ready());
    }
    #[test]
    fn retained_dependents_block_drop() {
        let mut a = view("a", "SELECT * FROM b");
        let b = view("b", "SELECT 1");
        a.dependencies.push(b.key());
        let s = snap(vec![a.clone()]);
        let t = snap(vec![a, b]);
        let mut d = compare(&s, &t, &Options::default());
        for x in &mut d {
            x.selected = true;
        }
        assert!(!plan(
            &s,
            &t,
            &d,
            &Options {
                allow_drops: true,
                ..Options::default()
            }
        )
        .ready());
    }
    #[test]
    fn case_sensitive_literals() {
        let s = snap(vec![view("a", "SELECT 'A'")]);
        let t = snap(vec![view("a", "SELECT 'a'")]);
        assert_eq!(
            compare(&s, &t, &Options::default())[0].status,
            Status::Different
        );
    }
}
