use crate::{
    engine::Options,
    model::{Kind, Object},
    sql::column_sql,
};
use similar::{ChangeTag, TextDiff};

/// Normalize the table-column display only. Captured snapshots and deployment
/// SQL keep their original definitions; ignored order must not create yellow rows.
pub fn display_sql(object: &Object, options: &Options) -> String {
    if object.kind != Kind::Table || !options.ignore_column_order {
        return object.definition.clone();
    }
    let mut columns: Vec<_> = object.columns.iter().collect();
    columns.sort_by_key(|c| {
        if options.ignore_column_name_case {
            c.name.to_lowercase()
        } else {
            c.name.clone()
        }
    });
    let body = columns
        .iter()
        .map(|c| format!("    {}", column_sql(c)))
        .collect::<Vec<_>>()
        .join(",\n");
    let children = object
        .definition
        .split_once("\nGO\n\n-- Constraints, indexes and DML triggers\n")
        .map(|(_, tail)| tail);
    let mut result = format!("CREATE TABLE {} (\n{}\n);", object.qualified(), body);
    if let Some(children) = children {
        result.push_str("\nGO\n\n-- Constraints, indexes and DML triggers\n");
        result.push_str(children);
    }
    result
}
#[derive(Clone, Debug)]
pub struct Line {
    pub number: usize,
    pub text: String,
    pub changed: bool,
}
#[derive(Clone, Debug)]
pub struct Row {
    pub source: Option<Line>,
    pub target: Option<Line>,
}
pub fn aligned(source: &str, target: &str) -> Vec<Row> {
    let diff = TextDiff::from_lines(target, source);
    let mut rows = Vec::new();
    let mut removed = Vec::new();
    let mut added = Vec::new();
    let flush = |rows: &mut Vec<Row>, added: &mut Vec<Line>, removed: &mut Vec<Line>| {
        let count = added.len().max(removed.len());
        let mut a = added.drain(..);
        let mut r = removed.drain(..);
        for _ in 0..count {
            rows.push(Row {
                source: a.next(),
                target: r.next(),
            });
        }
    };
    for change in diff.iter_all_changes() {
        let line = change.to_string().trim_end_matches(['\r', '\n']).to_owned();
        match change.tag() {
            ChangeTag::Delete => removed.push(Line {
                number: change.old_index().unwrap() + 1,
                text: line,
                changed: true,
            }),
            ChangeTag::Insert => added.push(Line {
                number: change.new_index().unwrap() + 1,
                text: line,
                changed: true,
            }),
            ChangeTag::Equal => {
                flush(&mut rows, &mut added, &mut removed);
                rows.push(Row {
                    source: Some(Line {
                        number: change.new_index().unwrap() + 1,
                        text: line.clone(),
                        changed: false,
                    }),
                    target: Some(Line {
                        number: change.old_index().unwrap() + 1,
                        text: line,
                        changed: false,
                    }),
                });
            }
        }
    }
    flush(&mut rows, &mut added, &mut removed);
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inserted_lines_keep_following_lines_aligned() {
        let r = aligned("a\ninserted\nb\n", "a\nb\n");
        assert_eq!(r.len(), 3);
        assert!(r[1].target.is_none());
        assert_eq!(r[2].source.as_ref().unwrap().number, 3);
        assert_eq!(r[2].target.as_ref().unwrap().number, 2);
    }
    #[test]
    fn replacements_pair_both_sides() {
        let r = aligned("a\nnew\nz\n", "a\nold\nz\n");
        assert!(r[1].source.as_ref().unwrap().changed);
        assert!(r[1].target.as_ref().unwrap().changed);
        assert!(!r[2].source.as_ref().unwrap().changed);
    }
}
