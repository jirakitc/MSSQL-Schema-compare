use crate::{engine::Options, model::Object, sql::normalized};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnMapping {
    pub source: String,
    pub target: String,
}

fn name_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn similarity(a: &str, b: &str) -> f32 {
    let a: Vec<_> = name_key(a).chars().collect();
    let b: Vec<_> = name_key(b).chars().collect();
    if a.len().min(b.len()) < 4 {
        return 0.0;
    }
    let mut row: Vec<_> = (0..=b.len()).collect();
    for (i, ac) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, bc) in b.iter().enumerate() {
            let previous = row[j + 1];
            row[j + 1] = (row[j] + 1)
                .min(previous + 1)
                .min(diagonal + usize::from(ac != bc));
            diagonal = previous;
        }
    }
    1.0 - row[b.len()] as f32 / a.len().max(b.len()) as f32
}

/// Exact names take priority. Only mutually unambiguous, similarly named,
/// compatible columns are mapped; ambiguous pairs remain ordinary drop/add changes.
pub fn auto_column_mappings(
    source: &Object,
    target: &Object,
    options: &Options,
) -> Vec<ColumnMapping> {
    if !options.auto_map_similar_columns {
        return vec![];
    }
    let same_name = |a: &str, b: &str| {
        if options.ignore_column_name_case {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let source_columns: Vec<_> = source
        .columns
        .iter()
        .filter(|c| !target.columns.iter().any(|t| same_name(&c.name, &t.name)))
        .collect();
    let target_columns: Vec<_> = target
        .columns
        .iter()
        .filter(|c| !source.columns.iter().any(|s| same_name(&c.name, &s.name)))
        .collect();
    let mut scores = vec![];
    for (si, s) in source_columns.iter().enumerate() {
        for (ti, t) in target_columns.iter().enumerate() {
            if normalized(&s.data_type, true) != normalized(&t.data_type, true)
                || s.identity != t.identity
                || s.computed.is_some()
                || t.computed.is_some()
                || (!options.ignore_collation && s.collation != t.collation)
            {
                continue;
            }
            let score = similarity(&s.name, &t.name);
            if score >= 0.8 {
                scores.push((si, ti, score));
            }
        }
    }
    scores
        .iter()
        .filter(|&&(si, ti, score)| {
            !scores.iter().any(|&(other_si, other_ti, other_score)| {
                (si != other_si || ti != other_ti)
                    && (si == other_si || ti == other_ti)
                    && other_score >= score - 0.1
            })
        })
        .map(|&(si, ti, _)| ColumnMapping {
            source: source_columns[si].name.clone(),
            target: target_columns[ti].name.clone(),
        })
        .collect()
}
