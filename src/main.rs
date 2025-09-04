use std::{fs, io};

use clap::Parser;
use config::Config;

mod config;
#[cfg(feature = "launch")]
mod desktop_entries;
mod state;

#[derive(clap::Parser)]
#[command(version, about)]
struct Cli {
    #[arg(short, long)]
    config: Option<String>,
    #[arg(short, long = "var")]
    variables: Vec<String>,
    #[arg(short = 'n', long)]
    max_items: Option<usize>,
    #[arg(long)]
    cold_run: bool,
}

fn main() {
    let cli = Cli::parse();

    match try_make_config(&cli) {
        Ok(mut config) => {
            config
                .variables
                .extend(cli.variables.into_iter().filter_map(|mut a| {
                    let index = a.find('=')?;
                    let b = a[index + 1..].to_string();
                    a.truncate(index);
                    Some((a, b))
                }));

            let apps = if config.general.search_apps {
                #[cfg(feature = "launch")]
                {
                    match desktop_entries::get_desktop_entries() {
                        Ok(items) => Some(items),
                        Err(e) => {
                            println!(r#"{{"type":"error","message":"{e}"}}"#);
                            return;
                        }
                    }
                }
                #[cfg(not(feature = "launch"))]
                {
                    eprintln!(
                        "superspace was not compiled with launcher support, but this config requires it. add use_launcher = false to the config to disable launcher support."
                    );
                    return;
                }
            } else {
                None
            };

            let mut state = state::State::new(&config, apps.as_ref(), cli.cold_run, cli.max_items);
            let mut api_input = String::new();

            while !state.should_exit {
                println!("{state}");
                // dbg!(&state);

                api_input.clear();
                if let Err(_) = io::stdin().read_line(&mut api_input) {
                    eprintln!("failed to get input");
                    break;
                }
                match api_input.as_str() {
                    "backspace\n" => state.process_backspace(),
                    "enter\n" => state.process_enter(),
                    _ => state.process_input(api_input.chars().next().unwrap()),
                }
            }
        }
        Err(e) => {
            println!(r#"{{"type":"error","message":"{e}"}}"#)
        }
    }
}

fn try_make_config(cli: &Cli) -> Result<Config, String> {
    let buf;
    let path = if let Some(path) = &cli.config {
        path
    } else {
        buf = format!(
            "{}/superspace/config.toml",
            dirs::config_dir()
                .expect("no config directory")
                .to_string_lossy()
        );
        &buf
    };
    let file = fs::read_to_string(path).map_err(|_| String::from("failed to find config file."))?;
    toml::from_str::<Config>(&file).map_err(|e| e.message().to_string())
}
