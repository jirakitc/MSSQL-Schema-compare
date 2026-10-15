//! Compare database objects as rows, keeping table components together like SQL Compare.
use crate::{
    engine::{compare, Difference, Options, Status},
    model::{Kind, Object, Snapshot},
};
use std::collections::BTreeMap;

pub fn compare_objects(source: &Snapshot, target: &Snapshot, options: &Options) -> Vec<Difference> {
    let components = compare(source, target, options);
    let roots: BTreeMap<_, _> = components
        .iter()
        .filter(|d| matches!(d.object().kind, Kind::Table | Kind::View))
        .map(|d| {
            (
                (d.object().schema.clone(), d.object().name.clone()),
                d.key.clone(),
            )
        })
        .collect();
    let mut rows: BTreeMap<String, Vec<&Difference>> = BTreeMap::new();
    for d in &components {
        let o = d.object();
        let parent = o
            .parent
            .as_ref()
            .and_then(|name| roots.get(&(o.schema.clone(), name.clone())));
        rows.entry(parent.unwrap_or(&d.key).clone())
            .or_default()
            .push(d);
    }
    rows.into_iter()
        .map(|(key, members)| {
            let root = members.iter().find(|d| d.key == key).unwrap();
            let status = match (&root.source, &root.target) {
                (Some(_), None) => Status::SourceOnly,
                (None, Some(_)) => Status::TargetOnly,
                _ if members.iter().all(|d| d.status == Status::Equal) => Status::Equal,
                _ => Status::Different,
            };
            let composite = |source: bool| -> Option<Object> {
                let mut o = if source {
                    root.source.clone()
                } else {
                    root.target.clone()
                }?;
                let children: Vec<_> = members
                    .iter()
                    .filter(|d| d.key != key)
                    .filter_map(|d| {
                        if source {
                            d.source.as_ref()
                        } else {
                            d.target.as_ref()
                        }
                    })
                    .collect();
                if !children.is_empty() {
                    o.definition
                        .push_str("\nGO\n\n-- Constraints, indexes and DML triggers\n");
                    for child in children {
                        o.definition.push_str(&child.definition);
                        o.definition.push_str("\nGO\n\n");
                    }
                }
                Some(o)
            };
            Difference {
                key: key.clone(),
                selected: matches!(status, Status::SourceOnly | Status::Different),
                status,
                source: composite(true),
                target: composite(false),
                components: members.iter().map(|d| d.key.clone()).collect(),
            }
        })
        .collect()
}
