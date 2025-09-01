use serde::{Deserialize, Deserializer, de};
use std::{collections::HashMap, path::PathBuf};

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub variables: HashMap<String, String>,
    #[serde(deserialize_with = "deserialize_commands")]
    pub command: HashMap<String, (UserCommand, usize)>,
}

pub(super) fn deserialize_commands<'de, D: Deserializer<'de, Error = E>, E: de::Error>(
    deser: D,
) -> Result<HashMap<String, (UserCommand, usize)>, D::Error> {
    let vec: Vec<UserCommand> = Vec::deserialize(deser)?;
    let mut map = HashMap::new();

    for (i, cmd) in vec.into_iter().enumerate() {
        if map.contains_key(&cmd.prefix) {
            return Err(E::custom(format!("duplicate command {}", cmd.prefix)));
        }
        map.insert(cmd.prefix.clone(), (cmd, i));
    }

    Ok(map)
}

#[derive(Default, Deserialize, Debug)]
pub struct GeneralConfig {
    pub default_command: Option<String>,
    pub prompt: Option<String>,
    #[serde(default = "search_apps_default")]
    pub search_apps: bool,
}

fn search_apps_default() -> bool {
    true
}

#[derive(Deserialize, Debug)]
pub struct Submenu {
    pub prompt: Option<String>,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Deserialize, Debug)]
pub struct UserCommand {
    pub prefix: String,
    pub description: String,
    pub action: Action,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    ListApplications,
    List {
        items: Vec<ListItem>,
    },
    Prompt {
        command: Vec<String>,
        #[serde(default)]
        output: OutputMode,
    },
    Exec {
        command: Vec<String>,
    },
    Exit,
    Submenu {
        name: String,
        #[serde(default)]
        variables: HashMap<String, String>,
    },
    #[serde(skip)]
    LaunchApp(PathBuf),
}

#[derive(Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    #[default]
    Hidden,
    Display,
    Continuous,
}

#[derive(Deserialize, Debug)]
pub struct ListItem {
    pub name: String,
    pub action: Action,
}

impl AsRef<str> for ListItem {
    fn as_ref(&self) -> &str {
        &self.name
    }
}
