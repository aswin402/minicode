//! Global lifecycle listeners, panic hooks, and RAII teardown guards
//! to guarantee zero-orphan process leaks on minicode exit.

use crate::dev::registry::{get_global_dev_registry, kill_all_sync};
use std::sync::atomic::{AtomicBool, Ordering};

static LIFECYCLE_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// RAII teardown guard that guarantees all running processes are terminated when dropped.
#[derive(Debug, Default)]
pub struct DevLifecycleGuard;

impl Drop for DevLifecycleGuard {
    fn drop(&mut self) {
        kill_all_sync();
    }
}

/// Installs OS signal listeners (SIGINT, SIGTERM, SIGHUP) and panic hooks
/// to terminate all managed processes on abnormal or normal shutdown.
pub fn install_lifecycle_hooks() -> DevLifecycleGuard {
    if !LIFECYCLE_INITIALIZED.swap(true, Ordering::SeqCst) {
        // 1. Install panic hook wrapper to kill processes synchronously on crash
        let prev_panic_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            kill_all_sync();
            prev_panic_hook(info);
        }));

        // 2. Spawn async signal listener if running inside an active tokio runtime
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                #[cfg(unix)]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut sigterm = signal(SignalKind::terminate()).ok();
                    let mut sighup = signal(SignalKind::hangup()).ok();

                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            tracing::info!("Received Ctrl+C, terminating all managed development processes...");
                        }
                        _ = async {
                            if let Some(ref mut s) = sigterm {
                                s.recv().await;
                            } else {
                                std::future::pending::<()>().await;
                            }
                        } => {
                            tracing::info!("Received SIGTERM, terminating all managed development processes...");
                        }
                        _ = async {
                            if let Some(ref mut s) = sighup {
                                s.recv().await;
                            } else {
                                std::future::pending::<()>().await;
                            }
                        } => {
                            tracing::info!("Received SIGHUP, terminating all managed development processes...");
                        }
                    }
                }

                #[cfg(not(unix))]
                {
                    let _ = tokio::signal::ctrl_c().await;
                    tracing::info!("Received Ctrl+C, terminating all managed development processes...");
                }

                let _ = get_global_dev_registry().kill_all().await;
            });
        }
    }

    DevLifecycleGuard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_guard_drop_is_safe() {
        let guard = DevLifecycleGuard;
        drop(guard);
    }

    #[tokio::test]
    async fn test_install_lifecycle_hooks_idempotency() {
        let _g1 = install_lifecycle_hooks();
        let _g2 = install_lifecycle_hooks();
        assert!(LIFECYCLE_INITIALIZED.load(Ordering::SeqCst));
    }
}
