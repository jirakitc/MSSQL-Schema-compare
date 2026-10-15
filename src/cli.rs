use anyhow::Result;
use clap::{Parser, Subcommand};
use schema_compare::{
    catalog::{capture, Connection},
    comparison::compare_objects as compare,
    engine::{plan, Options},
    model::Snapshot,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "SQL Server schema compare. No licensing service. Capture is read-only."
)]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Execute selected changes after checking that Source and Target match reviewed snapshots.
    Deploy {
        project: PathBuf,
        source: PathBuf,
        target: PathBuf,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Import the connections of an open or saved Redgate project; credentials stay in memory.
    ImportRedgate {
        project: Option<PathBuf>,
        #[arg(long)]
        active: bool,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        compare: bool,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(long)]
        databases: bool,
    },
    /// Fetch database names using Windows auth or SQLCMDPASSWORD, without choosing a database.
    Databases {
        #[arg(long)]
        server: String,
        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        trust_certificate: bool,
    },
    /// Capture a database with Microsoft ODBC sqlcmd. SQL password comes from SQLCMDPASSWORD.
    Capture {
        #[arg(long)]
        server: String,
        #[arg(long)]
        database: String,
        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        trust_certificate: bool,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Compare snapshots; optional script is blocked if migration is not supported.
    Compare {
        source: PathBuf,
        target: PathBuf,
        #[arg(long)]
        script: Option<PathBuf>,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(long)]
        ignore_comments: bool,
        #[arg(long)]
        allow_drops: bool,
    },
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    match Cli::parse().command {
        Action::Deploy {
            project,
            source,
            target,
            report,
        } => {
            let project = schema_compare::project::Project::read(&project)?;
            let source = Snapshot::read(&source)?;
            let target = Snapshot::read(&target)?;
            let selection = compare(&source, &target, &project.options);
            let outcome = schema_compare::deployment::deploy_and_refresh(
                &project,
                &source,
                &target,
                &selection,
                |stage| println!("{stage}"),
            );
            outcome.result.map_err(anyhow::Error::msg)?;
            let (source, after) = outcome.comparison.ok_or_else(|| {
                anyhow::anyhow!("Deployment completed but comparison is unavailable")
            })?;
            let differences = compare(&source, &after, &project.options);
            if let Some(path) = report {
                std::fs::write(
                    path,
                    serde_json::to_string_pretty(
                        &serde_json::json!({"source":source.label,"target":after.label,"differences":differences}),
                    )?,
                )?;
            }
            println!("Deployment complete; selected changes verified.");
        }
        Action::ImportRedgate {
            project,
            active,
            output,
            compare: run_compare,
            report,
            databases,
        } => {
            let path = if active {
                schema_compare::project::active_redgate_project()?
            } else {
                project.ok_or_else(|| anyhow::anyhow!("Provide a .scp file or --active"))?
            };
            let project = schema_compare::project::import_redgate(&path, true)?;
            println!(
                "Source: {} / {}",
                project.source.connection.server, project.source.connection.database
            );
            println!(
                "Target: {} / {}",
                project.target.connection.server, project.target.connection.database
            );
            println!(
                "Saved credentials loaded: source={}, target={}",
                !project.source.connection.password.is_empty(),
                !project.target.connection.password.is_empty()
            );
            for note in &project.import_notes {
                println!("Note: {note}");
            }
            if let Some(path) = output {
                project.write(&path)?;
            }
            if databases {
                for (side, endpoint) in [("Source", &project.source), ("Target", &project.target)] {
                    let names = schema_compare::catalog::list_databases(&endpoint.connection)?;
                    println!(
                        "{side}: {} accessible databases; selected database available={}",
                        names.len(),
                        names.contains(&endpoint.connection.database)
                    );
                }
            }
            if run_compare {
                let (s, t) = schema_compare::loading::load_pair(
                    &project.source,
                    &project.target,
                    |side, stage| println!("{side}: {stage}"),
                )?;
                println!("Source catalog: {} objects", s.objects.len());
                println!("Target catalog: {} objects", t.objects.len());
                let differences = compare(&s, &t, &project.options);
                println!(
                    "Different: {} | Source only: {} | Target only: {} | Equal: {}",
                    differences
                        .iter()
                        .filter(|d| d.status == schema_compare::engine::Status::Different)
                        .count(),
                    differences
                        .iter()
                        .filter(|d| d.status == schema_compare::engine::Status::SourceOnly)
                        .count(),
                    differences
                        .iter()
                        .filter(|d| d.status == schema_compare::engine::Status::TargetOnly)
                        .count(),
                    differences
                        .iter()
                        .filter(|d| d.status == schema_compare::engine::Status::Equal)
                        .count()
                );
                if let Some(path) = report {
                    std::fs::write(
                        path,
                        serde_json::to_string_pretty(
                            &serde_json::json!({"source":s.label,"target":t.label,"differences":differences}),
                        )?,
                    )?;
                }
            }
        }
        Action::Databases {
            server,
            username,
            trust_certificate,
        } => {
            let connection = Connection {
                server,
                windows_auth: username.is_none(),
                username: username.unwrap_or_default(),
                password: std::env::var("SQLCMDPASSWORD").unwrap_or_default(),
                trust_certificate,
                ..Default::default()
            };
            for name in schema_compare::catalog::list_databases(&connection)? {
                println!("{name}");
            }
        }
        Action::Capture {
            server,
            database,
            username,
            trust_certificate,
            output,
        } => {
            let c = Connection {
                server,
                database,
                windows_auth: username.is_none(),
                username: username.unwrap_or_default(),
                password: std::env::var("SQLCMDPASSWORD").unwrap_or_default(),
                trust_certificate,
            };
            let s = capture(&c)?;
            s.write(&output)?;
            println!(
                "Captured {} objects to {}",
                s.objects.len(),
                output.display()
            );
            for w in &s.warnings {
                println!("Scope: {w}");
            }
        }
        Action::Compare {
            source,
            target,
            script,
            report,
            ignore_comments,
            allow_drops,
        } => {
            let s = Snapshot::read(&source)?;
            let t = Snapshot::read(&target)?;
            let options = Options {
                ignore_comments,
                allow_drops,
                ..Options::default()
            };
            let mut diffs = compare(&s, &t, &options);
            if allow_drops {
                for d in &mut diffs {
                    d.selected = d.status != schema_compare::engine::Status::Equal;
                }
            }
            for d in &diffs {
                println!(
                    "{:<12} {} {}",
                    d.status.to_string(),
                    d.object().kind,
                    d.object().qualified()
                );
            }
            let p = plan(&s, &t, &diffs, &options);
            if let Some(path) = report {
                std::fs::write(
                    path,
                    serde_json::to_string_pretty(
                        &serde_json::json!({"source":s.label,"target":t.label,"differences":diffs,"plan":p}),
                    )?,
                )?;
            }
            if let Some(path) = script {
                if !p.blockers.is_empty() {
                    anyhow::bail!("Script blocked:\n{}", p.blockers.join("\n"));
                }
                if p.actions == 0 {
                    anyhow::bail!("No selected differences to script");
                }
                schema_compare::model::write_script(&path, &p.sql)?;
                println!("Saved {} actions to {}", p.actions, path.display());
            }
        }
    }
    Ok(())
}
