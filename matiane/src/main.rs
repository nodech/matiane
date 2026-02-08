use anyhow::Context;
use log::LevelFilter;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{
    arg,
    builder::{PossibleValuesParser, TypedValueParser},
    command, value_parser,
};

use matiane_core::log::init_global_logger;
use matiane_core::xdg::Xdg;

mod app;
mod config;
mod icon;
mod screen;

use app::App;

fn main() -> anyhow::Result<()> {
    let xdg = Xdg::new(matiane_core::NAME.into());

    let ParsedArgs {
        config_file,
        log_level,
    } = parse_args(&xdg);

    init_global_logger(log_level)?;

    let config = load_config(&config_file)?;

    let app_init = move || App::new(config.clone());

    iced::application(app_init, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .font(icon::FONT)
        .run()?;

    Ok(())
}

struct ParsedArgs {
    config_file: PathBuf,
    log_level: LevelFilter,
}

fn parse_args(xdg: &Xdg) -> ParsedArgs {
    let possible_levels: Vec<_> =
        LevelFilter::iter().map(|v| v.as_str()).collect();

    let matches = command!("Sway matiane gui")
        .arg(
            arg!(-c --config <FILE> "Sets a custom config file")
                .value_parser(value_parser!(PathBuf)),
        )
        .arg(
            arg!(-l --level <LEVEL> "Sets a log level")
                .value_parser(
                    PossibleValuesParser::new(possible_levels)
                        .map(|s| LevelFilter::from_str(&s).unwrap()),
                )
                .ignore_case(true)
                .default_value("WARN"),
        )
        .get_matches();

    let log_level = *matches.get_one::<LevelFilter>("level").unwrap();
    let config_file = matches
        .get_one::<PathBuf>("config")
        .cloned()
        .unwrap_or_else(|| xdg.config_dir().join("config.toml"));

    ParsedArgs {
        config_file,
        log_level,
    }
}

fn load_config(file: &PathBuf) -> anyhow::Result<config::MatianeConfig> {
    let file_str = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(config::MatianeConfig::default());
        }
        Err(e) => return Err(e).context("Failed to read configuration file"),
    };

    let parsed = toml::from_str::<config::MatianeConfig>(&file_str)
        .context("Failed to parse TOML from configuration file")?;

    Ok(parsed)
}
