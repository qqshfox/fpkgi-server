use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::Path;

use anyhow::Result;
use serde_json::{Value as JsonValue, from_reader, to_value};
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

fn merge_json_values(base: &mut JsonValue, external: JsonValue) {
    match (base, external) {
        (JsonValue::Object(base_map), JsonValue::Object(ext_map)) => {
            for (key, ext_value) in ext_map {
                if let Some(base_value) = base_map.get_mut(&key) {
                    merge_json_values(base_value, ext_value);
                } else {
                    base_map.insert(key, ext_value);
                }
            }
        }
        (base, external) => {
            *base = external;
        }
    }
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
            let rel_dir = path.parent()
                .unwrap_or(Path::new(""))
                .strip_prefix(pkg_fs_root)
                .unwrap_or(Path::new(""));
            let icon_name = format!("{}.png", path.file_name().unwrap().to_string_lossy());
            let icon_rel_path = rel_dir.join(&icon_name);
            let encoded_icon_rel_path = utf8_percent_encode(&icon_rel_path.to_string_lossy(), CONTROLS_WITH_SPACE).to_string();
            let icon_fullpath = icon_fs_root.join(&icon_rel_path);

            if let Some(parent) = icon_fullpath.parent() {
                fs::create_dir_all(parent)?;
            }

            if let Err(e) = pkg.save_file("icon0.png", &icon_fullpath) {
                info!("No icon extracted for '{}': {}", path.display(), e);
            }
            debug!("Extracted icon to '{}'", icon_fullpath.display());
            Some(format!("{}/{}", icon_url_root, encoded_icon_rel_path))
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

    if let Some(external_dir) = &args.external {
        for entry in WalkDir::new(external_dir).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().map_or(true, |ext| ext != "json") {
                continue;
            }

            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let category = file_name.strip_suffix(".json").unwrap_or(&file_name);
            if let Some(cat_data) = output_data.get_mut(category) {
                info!("Merging external JSON file: {}", path.display());
                let file = File::open(path)?;
                let external_json: JsonValue = from_reader(file)?;
                if let JsonValue::Object(external_json) = external_json {
                    if let Some(JsonValue::Object(data)) = external_json.get("DATA") {
                        let mut cat_data_value = to_value(cat_data.clone())?;
                        merge_json_values(&mut cat_data_value, JsonValue::Object(data.clone()));
                        if let JsonValue::Object(updated_map) = cat_data_value {
                            *cat_data = updated_map.into_iter().map(|(k, v)| {
                                (k, v.as_object().unwrap().clone().into_iter().collect())
                            }).collect();
                        }
                    }
                }
            } else {
                info!("Adding new category from external JSON: {}", path.display());
                let file = File::open(path)?;
                let external_json: JsonValue = from_reader(file)?;
                if let JsonValue::Object(external_json) = external_json {
                    if let Some(JsonValue::Object(data)) = external_json.get("DATA") {
                        let data_map: HashMap<String, HashMap<String, JsonValue>> = data.clone().into_iter()
                            .map(|(k, v)| (k, v.as_object().unwrap().clone().into_iter().collect()))
                            .collect();
                        output_data.insert(category.to_string(), data_map);
                    }
                }
            }
        }
    }

    Ok(output_data)
}
