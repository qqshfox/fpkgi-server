use anyhow::{Result, Context};
use log::{info, warn, error, debug}; // Added debug import
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

/// Watches filesystem changes in specified directories recursively.
///
/// Logs events such as file creation, modification, removal, and access using the `log` crate.
pub struct Watcher {
    _watcher: RecommendedWatcher, // Keeps the watcher alive
    receiver: Receiver<notify::Result<notify::Event>>, // Receives filesystem events
}

impl Watcher {
    /// Initializes a new watcher for the specified directories.
    pub fn new(dirs: Vec<PathBuf>) -> Result<Self> {
        let (sender, receiver) = channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| { let _ = sender.send(res); },
            Config::default()
                .with_poll_interval(std::time::Duration::from_secs(2))
                .with_compare_contents(false),
        ).context("Failed to create filesystem watcher")?;

        for dir in dirs {
            if dir.exists() && dir.is_dir() {
                watcher.watch(&dir, RecursiveMode::Recursive)
                    .with_context(|| format!("Failed to watch directory: {:?}", dir))?;
                info!("Watching directory: {:?}", dir);
            } else {
                warn!("Skipping invalid path: {:?}", dir);
            }
        }

        Ok(Watcher { _watcher: watcher, receiver })
    }

    /// Runs the watcher indefinitely, logging filesystem events.
    pub async fn run(self) -> Result<()> {
        while let Ok(event_result) = self.receiver.recv() {
            match event_result {
                Ok(event) => match event.kind {
                    notify::EventKind::Create(_) => info!("File created: {:?}", event.paths),
                    notify::EventKind::Modify(_) => info!("File modified: {:?}", event.paths),
                    notify::EventKind::Remove(_) => info!("File removed: {:?}", event.paths),
                    notify::EventKind::Access(_) => info!("File accessed: {:?}", event.paths),
                    _ => info!("Other event: {:?}", event),
                },
                Err(e) => error!("Watcher error: {:?}", e),
            }
        }
        error!("Watcher channel closed");
        Ok(())
    }

    /// Runs the watcher and re-runs generate on filesystem events.
    pub async fn run_with_generate(self, args: crate::args::GenerateArgs) -> Result<()> {
        while let Ok(event_result) = self.receiver.recv() {
            match event_result {
                Ok(event) => {
                    match event.kind {
                        notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_) => {
                            debug!("Filesystem event triggering regeneration: {:?}", event);
                            if let Err(e) = crate::run_generate(args.clone()).await {
                                error!("Failed to regenerate JSON files: {:?}", e);
                            } else {
                                info!("Regenerated JSON files due to filesystem change");
                            }
                        }
                        notify::EventKind::Access(_) => {
                            debug!("File accessed event ignored: {:?}", event.paths);
                        }
                        _ => debug!("Other event ignored: {:?}", event),
                    }
                }
                Err(e) => error!("Watcher error: {:?}", e),
            }
        }
        error!("Watcher channel closed");
        Ok(())
    }
}
