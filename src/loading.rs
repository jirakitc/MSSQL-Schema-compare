//! Independent Source/Target captures run concurrently; progress is delivered on the caller thread.
use crate::{model::Snapshot, project::Endpoint};
use anyhow::{anyhow, bail, Result};
use std::sync::mpsc::{self, Sender};

type Progress = (&'static str, String);
pub fn load_pair(
    source: &Endpoint,
    target: &Endpoint,
    progress: impl FnMut(&str, &str),
) -> Result<(Snapshot, Snapshot)> {
    parallel_pair(
        |tx| {
            let snapshot = source.load_with_progress(|stage| {
                let _ = tx.send(("Source", stage.into()));
            })?;
            let _ = tx.send((
                "Source",
                format!("Complete: {} catalog objects", snapshot.objects.len()),
            ));
            Ok(snapshot)
        },
        |tx| {
            let snapshot = target.load_with_progress(|stage| {
                let _ = tx.send(("Target", stage.into()));
            })?;
            let _ = tx.send((
                "Target",
                format!("Complete: {} catalog objects", snapshot.objects.len()),
            ));
            Ok(snapshot)
        },
        progress,
    )
}
fn parallel_pair<T: Send, U: Send>(
    source: impl FnOnce(Sender<Progress>) -> Result<T> + Send,
    target: impl FnOnce(Sender<Progress>) -> Result<U> + Send,
    mut progress: impl FnMut(&str, &str),
) -> Result<(T, U)> {
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();
        let source_tx = tx.clone();
        let a = scope.spawn(move || source(source_tx));
        let b = scope.spawn(move || target(tx));
        for (side, stage) in rx {
            progress(side, &stage);
        }
        let a = a
            .join()
            .map_err(|_| anyhow!("Source loader stopped unexpectedly"))
            .and_then(|r| r);
        let b = b
            .join()
            .map_err(|_| anyhow!("Target loader stopped unexpectedly"))
            .and_then(|r| r);
        match (a, b) {
            (Ok(a), Ok(b)) => Ok((a, b)),
            (Err(a), Err(b)) => bail!("Source: {a:#}\nTarget: {b:#}"),
            (Err(e), _) => Err(e.context("Source capture failed")),
            (_, Err(e)) => Err(e.context("Target capture failed")),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    #[test]
    fn both_loaders_start_before_either_finishes_and_progress_keeps_its_side() {
        let barrier = Arc::new(Barrier::new(2));
        let other = barrier.clone();
        let mut stages = vec![];
        let result = parallel_pair(
            move |tx| {
                tx.send(("Source", "reading".into())).unwrap();
                barrier.wait();
                Ok(1)
            },
            move |tx| {
                tx.send(("Target", "reading".into())).unwrap();
                other.wait();
                Ok(2)
            },
            |side, stage| stages.push((side.to_owned(), stage.to_owned())),
        )
        .unwrap();
        assert_eq!(result, (1, 2));
        stages.sort();
        assert_eq!(
            stages,
            vec![
                ("Source".into(), "reading".into()),
                ("Target".into(), "reading".into())
            ]
        );
    }
    #[test]
    fn failures_from_both_sides_are_reported() {
        let result = parallel_pair::<(), ()>(
            |_| bail!("source offline"),
            |_| bail!("target offline"),
            |_, _| {},
        )
        .unwrap_err()
        .to_string();
        assert!(result.contains("source offline") && result.contains("target offline"));
    }
}
