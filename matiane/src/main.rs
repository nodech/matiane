use matiane_core::args;
use matiane_core::config::load as load_config;
use matiane_core::log::init_global_logger;
use matiane_core::xdg::Xdg;

mod app;
mod config;
mod icon;
mod screen;

use app::App;

fn main() -> anyhow::Result<()> {
    let xdg = Xdg::new(matiane_core::NAME.into());

    let (
        _,
        args::GeneralArgs {
            config_file,
            log_level,
        },
    ) = matiane_core::args::parse_args(
        &xdg,
        "Sway matiane gui",
        std::iter::empty::<clap::Arg>(),
    );

    init_global_logger(log_level)?;

    let cfg = load_config::<config::MatianeConfig>(&config_file)?;

    let app_init = move || App::new(cfg.clone());

    iced::application(app_init, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .font(icon::FONT)
        .run()?;

    Ok(())
}
