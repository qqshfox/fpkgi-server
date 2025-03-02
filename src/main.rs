use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use tokio::task;

mod sfo_processor;
mod ps4_package;
mod enums;
mod utils;
mod json_builder;
mod args;
mod server;
mod watcher;

use args::GenerateArgs;
use json_builder::handle_packages;
use server::{run_server, ServerConfig};

#[derive(Parser)]
#[command(about = "FPKGi Server", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate JSON files from PS4 packages
    Generate(GenerateArgs),
    /// Start an HTTP server to serve directories
    Serve {
        /// List of directories in format name:path (e.g., packages:/path/to/dir)
        #[arg(long, required = true, num_args = 1..)]
        dirs: Vec<String>,
        /// Port to run server on (default: 8000)
        #[arg(long, default_value_t = 8000)]
        port: u16,
    },
    /// Watch directories for filesystem changes
    Watch {
        /// List of directories to watch (e.g., /path/to/dir)
        #[arg(long, required = true, num_args = 1..)]
        dirs: Vec<String>,
    },
    /// Host a server, generate JSONs, and regenerate on package changes in packages dir
    Host {
        /// Port to run server on (default: 8000)
        #[arg(long, default_value_t = 8000)]
        port: u16,
        /// Arguments for generate (packages, url, out, icons)
        #[command(flatten)]
        generate_args: GenerateArgs,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Generate(args) => run_generate(args).await,
        Commands::Serve { dirs, port } => {
            let config = server::parse_config(dirs).map_err(|e| anyhow::anyhow!(e))?;
            run_server(config, port).await
        }
        Commands::Watch { dirs } => {
            let paths: Vec<PathBuf> = dirs.into_iter().map(PathBuf::from).collect();
            let watcher = watcher::Watcher::new(paths).context("Failed to initialize file watcher")?;
            watcher.run().await
        }
        Commands::Host { port, generate_args } => {
            let mut directories = vec![
                (generate_args.packages.1.clone(), generate_args.packages.0.clone()),
                (generate_args.out.1.clone(), generate_args.out.0.clone()),
            ];
            if let Some((icons_fs_path, icons_url_path)) = &generate_args.icons {
                directories.push((icons_url_path.clone(), icons_fs_path.clone()));
            }

            let config = ServerConfig::new(directories.into_iter().collect());
            let watch_path = vec![generate_args.packages.0.clone()];

            // Generate initial JSON files
            run_generate(generate_args.clone()).await?;

            // Start the watcher in a separate task
            let watcher_handle = task::spawn(async move {
                let watcher = watcher::Watcher::new(watch_path)
                    .context("Failed to initialize file watcher")?;
                watcher.run_with_generate(generate_args).await?;
                Ok::<(), anyhow::Error>(())
            });

            // Run the server in the main task
            run_server(config, port).await?;

            // Wait for the watcher to complete (though it runs indefinitely)
            watcher_handle.await??;

            Ok(())
        }
    }
}

async fn run_generate(args: GenerateArgs) -> Result<()> {
    let processed_data = handle_packages(&args)?;

    let (json_fs_root, _) = &args.out;
    fs::create_dir_all(json_fs_root)?;
    for (category, entries) in processed_data {
        let json_file = json_fs_root.join(format!("{}.json", category));
        let mut file = File::create(&json_file)?;
        let json_data = serde_json::json!({"DATA": entries});
        let json_str = serde_json::to_string_pretty(&json_data)?;
        file.write_all(json_str.as_bytes())?;
        log::info!("Wrote {} data to {}", category, json_file.display());
    }
    Ok(())
}
