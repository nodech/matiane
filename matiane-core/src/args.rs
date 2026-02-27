use log::LevelFilter;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{
    Arg, ArgMatches, arg,
    builder::{PossibleValuesParser, TypedValueParser},
    command, value_parser,
};

use super::xdg::Xdg;

#[derive(Debug)]
pub struct GeneralArgs {
    pub config_file: PathBuf,
    pub log_level: LevelFilter,
}

pub fn general_args() -> impl IntoIterator<Item = impl Into<Arg>> {
    let possible_levels = LevelFilter::iter().map(|v| v.as_str());

    [
        arg!(-c --config <FILE> "Sets a custom config file")
            .value_parser(value_parser!(PathBuf)),
        arg!(-l --level <LEVEL> "Sets a log level")
            .value_parser(
                PossibleValuesParser::new(possible_levels)
                    .map(|s| LevelFilter::from_str(&s).unwrap()),
            )
            .ignore_case(true)
            .default_value("INFO"),
    ]
}

pub fn match_general_args(xdg: &Xdg, matches: &ArgMatches) -> GeneralArgs {
    let log_level = *matches.get_one::<LevelFilter>("level").unwrap();
    let config_file = matches
        .get_one::<PathBuf>("config")
        .cloned()
        .unwrap_or_else(|| xdg.config_dir().join("config.toml"));

    GeneralArgs {
        config_file,
        log_level,
    }
}

pub fn parse_args(
    xdg: &Xdg,
    name: &'static str,
    args: impl IntoIterator<Item = impl Into<Arg>>,
) -> (ArgMatches, GeneralArgs) {
    let matches = command!(name).args(general_args()).args(args).get_matches();

    let general_args = match_general_args(xdg, &matches);

    (matches, general_args)
}
