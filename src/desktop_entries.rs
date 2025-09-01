use std::{borrow::Cow, env, ffi::OsStr, fs};

use gio::{DesktopAppInfo, prelude::AppInfoExt};
use lazy_regex::regex_captures;

use crate::config::{Action, ListItem};

pub fn get_desktop_entries() -> Result<Vec<ListItem>, String> {
    let directories = env::var("XDG_DATA_DIRS").map_err(|e| {
        String::from(match e {
            env::VarError::NotPresent => "no applications found.",
            env::VarError::NotUnicode(_) => "invalid unicode in XDG_DATA_DIRS.",
        })
    })?;
    let directories = directories
        .split(':')
        .map(|d| String::from(d) + "/applications/");

    let mut entries = Vec::new();
    for dir in directories {
        let Ok(files) = fs::read_dir(&dir) else {
            continue;
        };

        for file in files {
            let Ok(file) = file else {
                eprintln!("skipped a file");
                continue;
            };
            let path = file.path();
            if path.extension().is_some_and(|o| o == "desktop") {
                let Some(info) = DesktopAppInfo::from_filename(&path) else {
                    eprintln!("skipped a file");
                    continue;
                };
                entries.push(ListItem {
                    name: info.name().to_string(),
                    action: Action::LaunchApp(path),
                });
            }
        }
    }

    Ok(entries)
}
