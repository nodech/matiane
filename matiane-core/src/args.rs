use log::LevelFilter;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{
    Arg, ArgMatches, arg,
    builder::{PossibleValuesParser, TypedValueParser},
    command, value_parser,
};

use super::xdg::Xdg;

pub struct GeneralArgs {
    pub config_file: PathBuf,
    pub log_level: LevelFilter,
}

pub fn parse_args(
    xdg: &Xdg,
    name: &'static str,
    args: impl IntoIterator<Item = impl Into<Arg>>,
) -> (ArgMatches, GeneralArgs) {
    let possible_levels = LevelFilter::iter().map(|v| v.as_str());

    let matches = command!(name)
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
                .default_value("INFO"),
        )
        .args(args)
        .get_matches();

    let log_level = *matches.get_one::<LevelFilter>("level").unwrap();
    let config_file = matches
        .get_one::<PathBuf>("config")
        .cloned()
        .unwrap_or_else(|| xdg.config_dir().join("config.toml"));

    (
        matches,
        GeneralArgs {
            config_file,
            log_level,
        },
    )
}
