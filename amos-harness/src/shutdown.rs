//! Graceful shutdown signal handling for the AMOS Harness.
//!
//! Implements AMOS-SECURE-002:
//! * Listens for `SIGTERM` and `SIGINT` on Unix, `Ctrl-C` on non-Unix.
//! * When a signal arrives, axum stops accepting new connections and waits
//!   for in-flight requests (including SSE streams) to drain.
//! * A 30-second watchdog task force-exits the process if the drain hangs,
//!   matching acceptance criterion (4) "Process exits within 30 seconds of
//!   signal" and preventing stuck rolling deploys.

use std::future::Future;
use std::time::Duration;
use tracing::{error, info};

/// Maximum time the server may spend draining in-flight requests before
/// the watchdog forces a hard exit. Tied to AMOS-SECURE-002 acceptance (4).
const DRAIN_DEADLINE: Duration = Duration::from_secs(30);

/// Future passed to `axum::serve(...).with_graceful_shutdown(...)`.
///
/// Resolves on the first OS shutdown signal, then spawns a detached
/// watchdog that will `std::process::exit(1)` if the drain takes longer
/// than [`DRAIN_DEADLINE`]. If the server drains cleanly and `main`
/// returns before the deadline, the runtime shuts down and the watchdog
/// is dropped without firing.
pub async fn shutdown_signal() {
    wait_for_os_signal().await;
    info!(
        "Shutdown signal received; starting graceful drain (max {:?})",
        DRAIN_DEADLINE
    );
    spawn_drain_watchdog(DRAIN_DEADLINE);
}

async fn wait_for_os_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        wait_any(
            async move {
                let _ = sigint.recv().await;
            },
            Some(async move {
                let _ = sigterm.recv().await;
            }),
        )
        .await;
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Resolve as soon as `primary` completes, or `secondary` if one is
/// provided and fires first. Split out so it can be unit-tested without
/// delivering real OS signals to the test process (which would
/// destabilize the parallel test runner).
async fn wait_any<F1, F2>(primary: F1, secondary: Option<F2>)
where
    F1: Future,
    F2: Future,
{
    match secondary {
        Some(secondary) => {
            tokio::select! {
                _ = primary => {},
                _ = secondary => {},
            }
        }
        None => {
            let _ = primary.await;
        }
    }
}

fn spawn_drain_watchdog(deadline: Duration) {
    tokio::spawn(async move {
        tokio::time::sleep(deadline).await;
        error!(
            "Graceful drain exceeded {:?}; forcing exit to avoid stuck deploys",
            deadline
        );
        std::process::exit(1);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_any_returns_when_primary_resolves_first() {
        // primary resolves immediately; secondary would take 60s.
        wait_any(async {}, Some(tokio::time::sleep(Duration::from_secs(60)))).await;
    }

    #[tokio::test]
    async fn wait_any_returns_when_secondary_resolves_first() {
        // primary never resolves; secondary fires immediately.
        wait_any(std::future::pending::<()>(), Some(async {})).await;
    }

    #[tokio::test]
    async fn wait_any_returns_when_only_primary_available() {
        // non-unix path: no secondary, primary resolves.
        wait_any::<_, std::future::Pending<()>>(async {}, None).await;
    }
}
