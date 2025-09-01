use std::{
    borrow::Cow,
    collections::HashMap,
    env::vars_os,
    ffi::{OsStr, OsString},
    fmt::Display,
    fs,
    process::{Command, Stdio},
};

use lazy_regex::regex_replace_all;

use nucleo::{
    Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

use crate::config::{Action, Config, ListItem, OutputMode, Submenu, UserCommand};

#[derive(Debug)]
pub(crate) enum StateEnum<'conf> {
    MainMenu {
        items: &'conf HashMap<String, (UserCommand, usize)>,
        filtered: Option<Vec<(&'conf String, usize)>>,
    },
    Prompt {
        prefix_len: usize,
        command: &'conf Vec<String>,
        output_mode: OutputMode,
    },
    List {
        prefix_len: usize,
        items: &'conf Vec<ListItem>,
        filtered: Option<Vec<(&'conf ListItem, u32)>>,
    },
    Error(String),
}

#[derive(Debug)]
pub(crate) struct State<'conf> {
    state_enum: StateEnum<'conf>,

    matcher: Matcher,
    input: String,
    prompt: &'conf Option<String>,

    pub should_exit: bool,
    cold_run: bool,

    temp_variables: HashMap<&'conf str, Cow<'conf, str>>,
    loaded_menus: HashMap<&'conf String, &'static Submenu>,
    apps: Option<&'conf Vec<ListItem>>,
    config: &'conf Config,
}

impl<'conf> State<'conf> {
    pub(crate) fn new(
        config: &'conf Config,
        apps: Option<&'conf Vec<ListItem>>,
        cold_run: bool,
    ) -> Self {
        Self {
            state_enum: StateEnum::MainMenu {
                items: &config.command,
                filtered: None,
            },

            matcher: Matcher::new(nucleo::Config::DEFAULT),
            input: String::new(),
            prompt: &config.general.prompt,

            should_exit: false,
            cold_run,

            temp_variables: HashMap::new(),
            loaded_menus: HashMap::new(),
            apps,
            config,
        }
    }

    pub(crate) fn process_input(&mut self, added_char: char) {
        self.input.push(added_char);
        match &mut self.state_enum {
            StateEnum::MainMenu { items, filtered } => {
                if added_char == ' ' {
                    let key = &self.input[..self.input.len() - 1];
                    let (cmd, _) = if let Some(cmd) = items.get(key) {
                        cmd
                    } else if let Some(filtered) = &*filtered {
                        if let Some((key, _)) = filtered.first() {
                            self.input = (*key).clone();
                            self.input.push(' ');
                            items.get(*key).expect("filtered results should be in map")
                        } else {
                            return; // no results = state doesn't change when you hit space
                        }
                    } else if let Some(default_cmd) = &self.config.general.default_command {
                        if let Some(cmd) = items.get(default_cmd) {
                            self.input = (*default_cmd).clone();
                            self.input.push(' ');
                            cmd
                        } else {
                            self.state_enum =
                                StateEnum::Error(format!("command '{default_cmd}' doesn't exist."));
                            return;
                        }
                    } else {
                        *filtered = None;
                        return;
                    };
                    self.run_cmd(&cmd.action);
                } else if filtered.as_ref().is_none_or(|f| !f.is_empty()) {
                    // if there already is no matches, there is not going to suddenly be more.
                    *filtered = Some(State::get_prefix_matches(&self.input, *items));
                }
            }
            StateEnum::List {
                items,
                filtered,
                prefix_len,
            } => {
                *filtered = Some(State::get_matches(
                    &self.input[*prefix_len..],
                    &mut self.matcher,
                    *items,
                ))
            }
            StateEnum::Prompt { .. } | StateEnum::Error(_) => (),
        }
    }

    pub(crate) fn process_delete(&mut self) {
        if self.input.pop().is_none() {
            return;
        }

        match &mut self.state_enum {
            StateEnum::MainMenu { items, filtered } => {
                if !self.input.is_empty() {
                    let mut results: Vec<_> = items
                        .iter()
                        .map(|(k, (_, i))| (k, *i))
                        .filter(|(k, _)| k.starts_with(&self.input))
                        .collect();
                    results.sort_by_key(|(_, i)| *i);
                    *filtered = Some(State::get_prefix_matches(&self.input, *items));
                } else {
                    *filtered = None;
                }
            }
            StateEnum::List {
                items,
                filtered,
                prefix_len,
            } => {
                if self.input.len() <= *prefix_len {
                    self.state_enum = StateEnum::MainMenu {
                        items: &self.config.command,
                        filtered: Some(State::get_prefix_matches(
                            &self.input,
                            self.config.command.iter(),
                        )),
                    }
                } else if !self.input.is_empty() {
                    *filtered = Some(State::get_matches(&self.input, &mut self.matcher, *items));
                } else {
                    *filtered = None;
                }
            }
            StateEnum::Prompt { prefix_len, .. } => {
                if self.input.len() <= *prefix_len {
                    self.state_enum = StateEnum::MainMenu {
                        items: &self.config.command,
                        filtered: Some(State::get_prefix_matches(
                            &self.input,
                            self.config.command.iter(),
                        )),
                    }
                }
            }
            StateEnum::Error(_) => (),
        }
    }

    pub(crate) fn process_enter(&mut self) {
        match &mut self.state_enum {
            StateEnum::MainMenu { items, filtered } => {
                if let Some(filtered) = &*filtered {
                    if let Some((prefix, _)) = filtered.first() {
                        let prefix = *prefix;
                        let (cmd, _) = items
                            .get(prefix)
                            .expect("filtered results should be in map");

                        if matches!(
                            cmd.action,
                            Action::List { .. } | Action::Prompt { .. } | Action::ListApplications
                        ) {
                            self.input = prefix.clone();
                            self.input.push(' ');
                        }

                        self.run_cmd(&cmd.action);
                    } else {
                        return; // no results = state doesn't change when you hit space
                    }
                } else {
                    return;
                }
            }
            StateEnum::List {
                items,
                filtered,
                prefix_len: _,
            } => {
                let item = if let Some((item, _)) = filtered.as_ref().and_then(|f| f.first()) {
                    *item
                } else {
                    match items.get(0) {
                        Some(i) => i,
                        None => return,
                    }
                };

                if matches!(
                    item.action,
                    Action::List { .. } | Action::Prompt { .. } | Action::ListApplications
                ) {
                    self.input
                        .truncate(self.input.find(' ').expect("list must have a prefix"));
                    self.input.push_str(&item.name);
                    self.input.push(' ');
                }

                self.run_cmd(&item.action);
            }
            StateEnum::Prompt {
                command,
                prefix_len,
                ..
            } => {
                let command = *command;

                let old_input = self
                    .temp_variables
                    .insert("INPUT", Cow::Owned(self.input[*prefix_len..].to_string()));
                if self.cold_run {
                    dbg!(command, &self.config.variables, &self.temp_variables);
                    self.should_exit = true;
                } else {
                    self.exec(command);
                }
                if let Some(old) = old_input {
                    self.temp_variables.insert("INPUT", old);
                }
            }
            StateEnum::Error(_) => {
                self.should_exit = true;
            }
        }
    }

    fn get_matches<'a, T: AsRef<str>>(
        input: &str,
        matcher: &mut Matcher,
        items: impl IntoIterator<Item = &'a T>,
    ) -> Vec<(&'a T, u32)> {
        let pattern = Pattern::new(
            &input,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        pattern.match_list(items, matcher)
    }
    fn get_prefix_matches(
        input: &str,
        items: impl IntoIterator<Item = (&'conf String, &'conf (UserCommand, usize))>,
    ) -> Vec<(&'conf String, usize)> {
        let mut results: Vec<_> = items
            .into_iter()
            .map(|(k, (_, i))| (k, *i))
            .filter(|(k, _)| k.starts_with(input))
            .collect();
        results.sort_by_key(|(_, i)| *i);
        results
    }

    fn run_cmd(&mut self, action: &'conf Action) {
        match action {
            Action::ListApplications => {
                self.state_enum = if let Some(items) = &self.apps {
                    StateEnum::List {
                        prefix_len: self.input.len(),
                        items: *items,
                        filtered: None,
                    }
                } else {
                    StateEnum::Error(String::from("applications are disabled in the config."))
                }
            }
            Action::List { items } => {
                self.state_enum = StateEnum::List {
                    prefix_len: self.input.len(),
                    items,
                    filtered: None,
                }
            }
            Action::Prompt { command, output } => {
                self.state_enum = StateEnum::Prompt {
                    command,
                    prefix_len: self.input.len(),
                    output_mode: *output,
                };
            }
            Action::Exec { command } => {
                if self.cold_run {
                    dbg!(command, &self.config.variables, &self.temp_variables);
                    self.should_exit = true;
                } else {
                    self.exec(command);
                }
            }
            Action::Submenu { name, variables } => {
                let changed_variables: Vec<(&'conf str, Cow<'conf, str>)> = variables
                    .iter()
                    .filter_map(|(k, v)| {
                        self.temp_variables
                            .insert(k, Cow::Borrowed(v))
                            .map(|o| (k.as_str(), o))
                    })
                    .collect();
                match self.load_menu(name) {
                    Err(e) => {
                        self.state_enum = StateEnum::Error(e);
                    }
                    Ok(submenu) => {
                        self.prompt = &submenu.prompt;
                        self.input = String::new();
                        self.state_enum = match &submenu.action {
                            Action::List { items } => StateEnum::List {
                                prefix_len: 0,
                                items,
                                filtered: None,
                            },
                            Action::Prompt { command, output } => StateEnum::Prompt {
                                command,
                                prefix_len: 0,
                                output_mode: *output,
                            },
                            _ => StateEnum::Error(format!(
                                "submenus must be a list or a prompt. (encountered in submenu '{name}')"
                            )),
                        }
                    }
                }
                self.temp_variables.extend(changed_variables.into_iter());
            }
            Action::Exit => self.should_exit = true,
            Action::LaunchApp(path_buf) => {
                #[cfg(feature = "launch")]
                {
                    use gio::{AppLaunchContext, DesktopAppInfo, prelude::AppInfoExt};

                    let Some(app) = DesktopAppInfo::from_filename(&path_buf) else {
                        return;
                    };
                    match app.launch(&[], None::<&AppLaunchContext>) {
                        Ok(()) => self.should_exit = true,
                        Err(e) => {
                            self.state_enum = StateEnum::Error(format!("failed to launch app: {e}"))
                        }
                    };
                }
                #[cfg(not(feature = "launch"))]
                {
                    self.state_enum = StateEnum::Error(String::from(
                        "superspace was compiled without launcher support.",
                    ));
                }
            }
        }
    }

    fn create_cmd_iter(
        config: &Config,
        temp_vars: &HashMap<&str, Cow<'conf, str>>,
        cmd: &'conf Vec<String>,
    ) -> impl Iterator<Item = Cow<'conf, OsStr>> {
        cmd.iter().map(move |s| {
            let str = regex_replace_all!(r"\{\{([\w]+)\}\}", s, |_, name| {
                if let Some(v) = temp_vars.get(name) {
                    v
                } else {
                    config.variables.get(name).map(String::as_str).unwrap_or("")
                }
            });

            match str {
                Cow::Borrowed(s) => Cow::Borrowed(OsStr::new(s)),
                Cow::Owned(s) => Cow::Owned(OsString::from(s)),
            }
        })
    }

    pub(crate) fn exec(&mut self, cmd: &Vec<String>) {
        let mut cmd_iter = State::create_cmd_iter(self.config, &self.temp_variables, cmd);

        if let Some(program) = cmd_iter.next() {
            match Command::new(program)
                .args(cmd_iter)
                .envs(&mut vars_os())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(_) => (),
                Err(e) => self.state_enum = StateEnum::Error(format!("{e}")),
            };
        }

        self.should_exit = true;
    }

    fn load_menu(&mut self, name: &'conf String) -> Result<&'static Submenu, String> {
        match self.loaded_menus.get(name) {
            None => {
                let Ok(file) = fs::read_to_string(format!(
                    "{}/superspace/{name}.toml",
                    dirs::config_dir().expect("no config dir").to_string_lossy()
                )) else {
                    return Err(format!("file not found: ~/.config/superspace/{name}.toml"));
                };
                match toml::from_str::<Submenu>(&file) {
                    // SAFETY: this is an on-purpose memory leak. for my purposes, there will only
                    // ever be 1 State, which will never be de-allocated until the end of the
                    // program, so leaking this memory is okay.
                    Ok(m) => Ok(Box::leak(Box::new(m))),
                    Err(e) => {
                        return Err(e.message().to_string());
                    }
                }
            }
            Some(a) => Ok(*a),
        }
    }
}

impl Display for State<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn sanitize(input: &str) -> Cow<'_, str> {
            regex_replace_all!(r#"["\\]"#, input, |c| String::from('\\') + c)
        }

        let input = sanitize(&self.input);
        let prompt = self
            .prompt
            .as_ref()
            .map(|p| format!(r#", "prompt": "{}""#, sanitize(p)));
        let prompt = prompt.as_ref().map(|s| s.as_str()).unwrap_or("");

        match &self.state_enum {
            StateEnum::MainMenu { items, filtered } => {
                fn stringify<'a>(items: impl Iterator<Item = &'a UserCommand>) -> String {
                    let mut iter = items.flat_map(|cmd| {
                        [
                            r#"{"prefix": ""#,
                            &cmd.prefix,
                            r#"","description":""#,
                            &cmd.description,
                            r#""},"#,
                        ]
                    });
                    let Some(mut string) = iter.next().map(String::from) else {
                        return String::new();
                    };
                    for chunk in iter {
                        string.push_str(chunk);
                    }
                    string.pop(); // remove last comma
                    string
                }
                let items = filtered.as_ref().map_or_else(
                    || {
                        let mut vec: Vec<_> = items.values().collect();
                        vec.sort_by_key(|(_, i)| *i);
                        stringify(vec.into_iter().map(|(v, _)| v))
                    },
                    |v| stringify(v.iter().filter_map(|(k, _)| items.get(*k).map(|(c, _)| c))),
                );
                write!(
                    f,
                    r#"{{"type":"main_menu","input":"{input}","items":[{items}]{prompt}}}"#,
                )
            }
            StateEnum::Prompt {
                prefix_len,
                command,
                output_mode,
            } => {
                let output =
                    if *output_mode == OutputMode::Continuous && self.input.len() > *prefix_len {
                        let mut tvars = self.temp_variables.clone();
                        tvars.insert("INPUT", Cow::Borrowed(&self.input[*prefix_len..]));
                        let mut iter = State::create_cmd_iter(self.config, &tvars, command);

                        if let Some(program) = iter.next() {
                            Command::new(program)
                                .args(iter)
                                .stdin(Stdio::null())
                                .output()
                                .ok()
                                .map(|output| {
                                    let output = String::from_utf8_lossy(&output.stdout);
                                    format!(r#","output":"{}""#, output.trim())
                                })
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                let output = output.as_ref().map(String::as_str).unwrap_or("");

                write!(
                    f,
                    r#"{{"type":"prompt","input":"{input}","prefix":"{prefix}"{output}{prompt}}}"#,
                    prefix = &input[..*prefix_len]
                )
            }
            StateEnum::List {
                prefix_len: _,
                items,
                filtered,
            } => {
                fn stringify<'a>(mut iter: impl Iterator<Item = &'a String>) -> String {
                    if let Some(first) = iter.next() {
                        let mut string = String::from('"');
                        string.push_str(first);
                        for item in iter {
                            string.push_str("\", \"");
                            string.push_str(item);
                        }
                        string.push('"');
                        string
                    } else {
                        String::new()
                    }
                }
                let items = filtered.as_ref().map_or_else(
                    || stringify(items.iter().map(|item| &item.name)),
                    |f| stringify(f.iter().map(|(item, _)| &item.name)),
                );

                write!(
                    f,
                    r#"{{"type":"list","input":"{input}","items":[{items}]{prompt}}}"#,
                )
            }
            StateEnum::Error(msg) => write!(f, r#"{{"type":"error","message":"{msg}"}}"#,),
        }
    }
}
