use std::{borrow::Cow, env, ffi::OsStr, fs};

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
                let Ok(contents) = fs::read_to_string(&path) else {
                    eprintln!("skipped a file");
                    continue;
                };
                let name = match regex_captures!(r#"\nName=([^\n]+)"#, &contents) {
                    Some((_, name)) => Cow::Borrowed(name),
                    None => path
                        .file_name()
                        .map(OsStr::to_string_lossy)
                        .unwrap_or(Cow::Borrowed("")),
                };
                entries.push(ListItem {
                    name: name.into_owned(),
                    action: Action::LaunchApp(path),
                });
            }
        }
    }

    Ok(entries)
}
