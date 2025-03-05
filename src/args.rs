use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser, Clone)]
pub struct GenerateArgs {
    /// Packages directory in format "fs_path:url_path"
    #[arg(long, value_parser = split_path_arg)]
    pub packages: (PathBuf, String),

    /// Base URL for package links
    #[arg(long)]
    pub url: String,

    /// Output directory in format "fs_path:url_path"
    #[arg(long, value_parser = split_path_arg)]
    pub out: (PathBuf, String),

    /// Optional icons directory in format "fs_path:url_path"
    #[arg(long, value_parser = split_path_arg)]
    pub icons: Option<(PathBuf, String)>,

    /// Optional external directory containing JSON files to merge
    #[arg(long)]
    pub external: Option<PathBuf>,
}

fn split_path_arg(value: &str) -> Result<(PathBuf, String), String> {
    if let Some((fs_part, url_part)) = value.split_once(':') {
        Ok((
            PathBuf::from(fs_part).canonicalize().unwrap_or_else(|_| PathBuf::from(fs_part)),
            url_part.to_string()
        ))
    } else {
        let path = PathBuf::from(value);
        Ok((
            path.clone().canonicalize().unwrap_or_else(|_| path.clone()),
            value.to_string()
        ))
    }
}
