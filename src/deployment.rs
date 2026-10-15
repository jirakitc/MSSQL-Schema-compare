use crate::{
    catalog::execute_deployment,
    comparison::compare_objects,
    engine::{plan, Difference, Status},
    model::{Object, Snapshot},
    project::Project,
};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};

pub struct DeploymentOutcome {
    pub result: Result<(), String>,
    pub comparison: Option<(Snapshot, Snapshot)>,
    pub refresh_error: Option<String>,
}
/// Always try to restore a current Compare after failure; never retry execution.
pub fn deploy_and_refresh(
    project: &Project,
    source: &Snapshot,
    target: &Snapshot,
    selection: &[Difference],
    mut progress: impl FnMut(&str),
) -> DeploymentOutcome {
    match deploy(project, source, target, selection, &mut progress) {
        Ok(pair) => DeploymentOutcome {
            result: Ok(()),
            comparison: Some(pair),
            refresh_error: None,
        },
        Err(error) => {
            progress("Deployment failed. Refreshing Source and Target for Compare");
            let refresh =
                crate::loading::load_pair(&project.source, &project.target, |side, stage| {
                    progress(&format!("{side}: {stage}"))
                });
            let (comparison, refresh_error) = match refresh {
                Ok(pair) => (Some(pair), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            };
            DeploymentOutcome {
                result: Err(format!("{error:#}")),
                comparison,
                refresh_error,
            }
        }
    }
}

pub fn validate_target(project: &Project, expected: &Snapshot) -> Result<()> {
    if project.target.snapshot {
        bail!("Deploy requires a live Target database; snapshot files cannot be deployed into");
    }
    if project.target.connection.database != expected.database {
        bail!("Target database changed; run Compare again");
    }
    if expected.label
        != format!(
            "{} / {}",
            project.target.connection.server, project.target.connection.database
        )
    {
        bail!("Target server changed; run Compare again");
    }
    Ok(())
}
pub fn validate_unchanged(expected: &Snapshot, current: &Snapshot, side: &str) -> Result<()> {
    if expected.database != current.database {
        bail!(
            "{side} database changed since Compare: {} -> {}. Run Compare again",
            expected.database,
            current.database
        );
    }
    expected.validate()?;
    current.validate()?;
    let before: BTreeMap<_, _> = expected.objects.iter().map(|o| (o.key(), o)).collect();
    let after: BTreeMap<_, _> = current.objects.iter().map(|o| (o.key(), o)).collect();
    let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
    let mut changes = Vec::new();
    for key in keys {
        match (before.get(key), after.get(key)) {
            (Some(a), Some(b)) => {
                let a_value = canonical_object(a)?;
                let b_value = canonical_object(b)?;
                if a_value != b_value {
                    let fields: Vec<_> = a_value
                        .as_object()
                        .into_iter()
                        .flat_map(|v| v.keys())
                        .filter(|name| a_value.get(*name) != b_value.get(*name))
                        .cloned()
                        .collect();
                    changes.push(format!(
                        "Modified {} ({})",
                        object_description(b),
                        fields.join(", ")
                    ));
                }
            }
            (None, Some(o)) => changes.push(format!("Added {}", object_description(o))),
            (Some(o), None) => changes.push(format!("Removed {}", object_description(o))),
            _ => {}
        }
    }
    if !changes.is_empty() {
        let count = changes.len();
        let details = changes
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let remainder = if count > 12 {
            format!("\n... and {} more changed objects", count - 12)
        } else {
            String::new()
        };
        bail!("{side} schema changed since Compare ({count} objects):\n{details}{remainder}\nReview the refreshed Compare before deploying");
    }
    Ok(())
}
fn canonical_object(object: &Object) -> Result<serde_json::Value> {
    let mut value = serde_json::to_value(object)?;
    // Dependencies are a set; columns and SQL definition remain ordered/exact.
    let dependencies: BTreeSet<_> = object.dependencies.iter().collect();
    value["dependencies"] = serde_json::to_value(dependencies)?;
    Ok(value)
}
fn object_description(object: &Object) -> String {
    if object.parent.is_some() {
        format!(
            "{} {} / {}",
            object.kind,
            object.parent_qualified(),
            object.name
        )
    } else {
        format!("{} {}", object.kind, object.qualified())
    }
}
pub fn deploy(
    project: &Project,
    expected_source: &Snapshot,
    expected_target: &Snapshot,
    selection: &[Difference],
    mut progress: impl FnMut(&str),
) -> Result<(Snapshot, Snapshot)> {
    validate_target(project, expected_target)?;
    let initial_plan = plan(
        expected_source,
        expected_target,
        selection,
        &project.options,
    );
    if !initial_plan.ready() {
        bail!(
            "Deployment blocked: {}",
            if initial_plan.blockers.is_empty() {
                "No selected changes".into()
            } else {
                initial_plan.blockers.join("\n")
            }
        );
    }
    progress("Preflight: checking Source and Target schemas concurrently");
    let (source, target) =
        crate::loading::load_pair(&project.source, &project.target, |side, stage| {
            progress(&format!("{side}: {stage}"))
        })?;
    validate_unchanged(expected_source, &source, "Source")?;
    validate_unchanged(expected_target, &target, "Target")?;
    let p = plan(&source, &target, selection, &project.options);
    if !p.ready() {
        bail!(
            "Deployment blocked: {}",
            if p.blockers.is_empty() {
                "No selected changes".into()
            } else {
                p.blockers.join("\n")
            }
        );
    }
    progress(&format!(
        "Executing {} schema actions on {} / {}",
        p.actions, project.target.connection.server, project.target.connection.database
    ));
    execute_deployment(&project.target.connection, &p.sql)?;
    progress("Deployment committed. Refreshing Source and Target concurrently for verification and Compare");
    let (current_source, after) = crate::loading::load_pair(
        &project.source,
        &project.target,
        |side, stage| progress(&format!("{side}: {stage}")),
    )
    .context(
        "Deployment committed, but comparison refresh could not finish. Refresh before retrying",
    )?;
    let results = compare_objects(&source, &after, &project.options);
    let remaining: Vec<_> = selection
        .iter()
        .filter(|d| d.selected && d.status != Status::Equal)
        .filter(|d| {
            results
                .iter()
                .any(|r| r.key == d.key && r.status != Status::Equal)
        })
        .map(|d| d.object().qualified())
        .collect();
    if !remaining.is_empty() {
        bail!("Deployment committed, but selected objects still differ: {}. Refresh and review before retrying",remaining.join(", "));
    }
    Ok((current_source, after))
}
