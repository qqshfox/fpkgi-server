use std::collections::HashMap;
use std::fs;

use anyhow::Result;
use serde_json::Value as JsonValue;
use log::{info, error, debug};
use walkdir::WalkDir;
use percent_encoding::{utf8_percent_encode, CONTROLS, AsciiSet};

use crate::args::GenerateArgs;
use crate::sfo_processor;
use crate::ps4_package::PS4Package;

const CATEGORY_MAP: &[(&str, &str)] = &[
    ("gd", "games"), ("gp", "updates"), ("ac", "DLC"), ("gde", "homebrew")
];

// Custom fragment set: CONTROLS plus space
const CONTROLS_WITH_SPACE: &AsciiSet = &CONTROLS.add(b' ');

fn build_json_schema<'a>(icon_link: Option<String>, pkg_bytes: u64) -> Vec<(Option<&'a str>, &'a str, Option<String>, Option<u64>)> {
    vec![
        (Some("TITLE_ID"), "title_id", None, None),
        (None, "region", None, None),
        (Some("TITLE"), "name", None, None),
        (Some("APP_VER"), "version", None, None),
        (None, "release", None, None),
        (None, "size", None, Some(pkg_bytes)),
        (None, "min_fw", None, None),
        (None, "cover_url", icon_link, None),
    ]
}

fn parse_region_from_content_id(content_id: &str) -> String {
    let region_code = content_id.get(0..2).unwrap_or("??").to_uppercase();
    match region_code.as_str() {
        "JP" => "JAP".to_string(),
        "UP" => "USA".to_string(),
        "EP" => "EUR".to_string(),
        _ => "UNK".to_string(),
    }
}

fn convert_sfo_to_json(base_link: &str, pkg_link: &str, pkg_bytes: u64, icon_path: Option<String>,
                       sfo_data: HashMap<String, String>, content_id: &str) -> (String, String, HashMap<String, JsonValue>) {
    let icon_link = icon_path.map(|p| format!("{}/{}", base_link, p));
    let mut json_output = HashMap::new();
    let region = parse_region_from_content_id(content_id);

    for (source, target, default_str, default_int) in build_json_schema(icon_link, pkg_bytes) {
        let value = if let Some(sfo_key) = source {
            sfo_data.get(sfo_key).cloned().map(JsonValue::String)
        } else if target == "region" {
            Some(JsonValue::String(region.clone()))
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

    for entry in WalkDir::new(pkg_fs_root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().map_or(true, |ext| ext != "pkg") {
            continue;
        }

        let pkg_bytes = fs::metadata(&path)?.len();
        let pkg_rel_path = path.strip_prefix(pkg_fs_root)?.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        let encoded_pkg_rel_path = utf8_percent_encode(&pkg_rel_path, CONTROLS_WITH_SPACE).to_string();
        let pkg_url_path = format!("{}/{}", pkg_url_root, encoded_pkg_rel_path);

        info!("Processing package: {} ({} bytes)", path.display(), pkg_bytes);

        let pkg = match PS4Package::new(path.to_path_buf()) {
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
            let encoded_icon_name = utf8_percent_encode(&icon_name, CONTROLS_WITH_SPACE).to_string();
            let icon_fullpath = icon_fs_root.join(&icon_name);
            if let Err(e) = pkg.save_file("icon0.png", &icon_fullpath) {
                info!("No icon extracted for '{}': {}", path.display(), e);
            }
            debug!("Extracted icon to '{}'", icon_fullpath.display());
            Some(format!("{}/{}", icon_url_root, encoded_icon_name))
        } else {
            None
        };

        let (cat, link, json_entry) = convert_sfo_to_json(
            &args.url,
            &pkg_url_path,
            pkg_bytes,
            icon_path,
            sfo_data,
            &pkg.content_id
        );
        let category = CATEGORY_MAP.iter().find(|&&(k, _)| k == cat).map(|&(_, v)| v).unwrap_or("games");
        output_data.get_mut(category).unwrap().insert(link, json_entry);
    }

    Ok(output_data)
}
