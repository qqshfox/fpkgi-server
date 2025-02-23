use std::collections::HashMap;
use std::fs;

use anyhow::Result;
use serde_json::Value as JsonValue;
use log::{info, error, debug};

use crate::args::GenerateArgs;
use crate::sfo_processor;
use crate::ps4_package::PS4Package;

const CATEGORY_MAP: &[(&str, &str)] = &[
    ("gd", "games"), ("gp", "updates"), ("ac", "DLC"), ("gde", "homebrew")
];

fn build_json_schema<'a>(icon_link: Option<String>, pkg_bytes: u64) -> Vec<(Option<&'a str>, &'a str, Option<String>, Option<u32>)> {
    vec![
        (Some("TITLE_ID"), "title_id", None, None),
        (None, "region", None, None),
        (Some("TITLE"), "name", None, None),
        (Some("APP_VER"), "version", None, None),
        (None, "release", None, None),
        (None, "size", None, Some(pkg_bytes as u32)),
        (None, "min_fw", None, None),
        (None, "cover_url", icon_link, None),
    ]
}

fn convert_sfo_to_json(base_link: &str, pkg_link: &str, pkg_bytes: u64, icon_path: Option<String>,
                       sfo_data: HashMap<String, String>) -> (String, String, HashMap<String, JsonValue>) {
    let icon_link = icon_path.map(|p| format!("{}/{}", base_link, p));
    let mut json_output = HashMap::new();

    for (source, target, default_str, default_int) in build_json_schema(icon_link, pkg_bytes) {
        let value = if let Some(sfo_key) = source {
            sfo_data.get(sfo_key).cloned().map(JsonValue::String)
        } else if target == "size" {
            default_int.map(|n| JsonValue::Number(serde_json::Number::from(n)))
        } else if let Some(s) = default_str {
            Some(JsonValue::String(s))
        } else {
            Some(JsonValue::Null)
        };
        json_output.insert(target.to_string(), value.unwrap_or(JsonValue::Null));
    }

    let category = sfo_data.get("CATEGORY").cloned().unwrap_or_else(|| "gd".to_string());
    (category, format!("{}/{}", base_link, pkg_link), json_output)
}

pub fn handle_packages(args: &GenerateArgs) -> Result<HashMap<String, HashMap<String, HashMap<String, JsonValue>>>> {
    let mut output_data: HashMap<String, HashMap<String, HashMap<String, JsonValue>>> =
        CATEGORY_MAP.iter().map(|(_, v)| (v.to_string(), HashMap::new())).collect();

    let (pkg_fs_root, pkg_url_root) = &args.packages;
    let icon_paths = args.icons.as_ref().map(|(fs, url)| (fs, url));
    let (_json_fs_root, _json_url_root) = &args.out;

    match fs::read_dir(pkg_fs_root) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map_or(true, |ext| ext != "pkg") {
                    continue;
                }

                let pkg_bytes = fs::metadata(&path)?.len();
                let pkg_rel_path = path.strip_prefix(pkg_fs_root)?.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
                let pkg_url_path = format!("{}/{}", pkg_url_root, pkg_rel_path);

                info!("Processing package: {} ({} bytes)", path.display(), pkg_bytes);

                let pkg = match PS4Package::new(path.clone()) {
                    Ok(pkg) => pkg,
                    Err(e) => {
                        error!("Failed to process package '{}': {}", path.display(), e);
                        continue;
                    }
                };

                let sfo_data = match sfo_processor::SFOProcessor::new().process(pkg.get_file("param.sfo").unwrap_or_default()) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("Failed to parse SFO for '{}': {}", path.display(), e);
                        continue;
                    }
                };

                let icon_path = if let Some((icon_fs_root, icon_url_root)) = icon_paths {
                    fs::create_dir_all(icon_fs_root)?;
                    let icon_name = format!("{}.png", path.file_name().unwrap().to_string_lossy());
                    let icon_fullpath = icon_fs_root.join(&icon_name);
                    if let Err(e) = pkg.save_file("icon0.png", &icon_fullpath) {
                        info!("No icon extracted for '{}': {}", path.display(), e);
                    }
                    debug!("Extracted icon to '{}'", icon_fullpath.display());
                    Some(format!("{}/{}", icon_url_root, icon_name))
                } else {
                    None
                };

                let (cat, link, json_entry) = convert_sfo_to_json(&args.url, &pkg_url_path, pkg_bytes, icon_path, sfo_data);
                let category = CATEGORY_MAP.iter().find(|&&(k, _)| k == cat).map(|&(_, v)| v).unwrap_or("games");
                output_data.get_mut(category).unwrap().insert(link, json_entry);
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to read packages directory '{}': {}", pkg_fs_root.display(), e));
        }
    }

    Ok(output_data)
}
