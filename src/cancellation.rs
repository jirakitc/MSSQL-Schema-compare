use anyhow::{bail, Context, Result};
use std::{
    process::{Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Acquire) {
            bail!("Comparison cancelled");
        }
        Ok(())
    }
}

/// Redirect pipes to temporary files so a full stderr pipe cannot stall cancellation.
/// Only the child process started by this call is terminated.
pub fn run_command(command: &mut Command, cancel: &CancellationToken) -> Result<Output> {
    cancel.check()?;
    let directory = tempfile::tempdir()?;
    let stdout = directory.path().join("stdout");
    let stderr = directory.path().join("stderr");
    command.stdout(Stdio::from(std::fs::File::create(&stdout)?));
    command.stderr(Stdio::from(std::fs::File::create(&stderr)?));
    let mut child = command
        .spawn()
        .context("Cannot start SQL Server command-line client")?;
    let status = loop {
        if let Err(error) = cancel.check() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
    };
    cancel.check()?;
    Ok(Output {
        status,
        stdout: std::fs::read(stdout)?,
        stderr: std::fs::read(stderr)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_load_never_starts_a_process_or_reads_a_snapshot() {
        let cancelled = CancellationToken::default();
        cancelled.cancel();
        let error = run_command(
            &mut Command::new("this-command-must-not-be-started"),
            &cancelled,
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        let endpoint = crate::project::Endpoint {
            snapshot: true,
            path: "missing-snapshot.json".into(),
            ..Default::default()
        };
        assert!(
            crate::loading::load_pair_cancellable(&endpoint, &endpoint, |_, _| {}, &cancelled)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
        assert!(CancellationToken::default().check().is_ok());
    }
    #[cfg(windows)]
    fn powershell(script: &str) -> Command {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x08000000);
        command
    }
    #[test]
    #[cfg(windows)]
    fn cancelling_stops_only_the_running_child_promptly() {
        let marker = tempfile::tempdir().unwrap();
        let path = marker.path().join("started");
        let mut command=powershell("Set-Content -LiteralPath $env:SC_CANCEL_TEST_MARKER -Value ready; Start-Sleep -Seconds 30");
        command.env("SC_CANCEL_TEST_MARKER", &path);
        let token = CancellationToken::default();
        let worker_token = token.clone();
        let worker = std::thread::spawn(move || run_command(&mut command, &worker_token));
        let started = std::time::Instant::now();
        while !path.exists() && started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let child_started = path.exists();
        let cancelled_at = std::time::Instant::now();
        token.cancel();
        let error = worker.join().unwrap().unwrap_err();
        assert!(child_started, "Test child did not start");
        assert!(error.to_string().contains("cancelled"));
        assert!(cancelled_at.elapsed() < Duration::from_secs(3));
    }
    #[test]
    #[cfg(windows)]
    fn normal_process_completion_preserves_status_and_diagnostics() {
        let mut command = powershell(
            "[Console]::Out.WriteLine('output'); [Console]::Error.WriteLine('diagnostic'); exit 3",
        );
        let output = run_command(&mut command, &CancellationToken::default()).unwrap();
        assert_eq!(output.status.code(), Some(3));
        assert!(String::from_utf8_lossy(&output.stdout).contains("output"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("diagnostic"));
    }
}
