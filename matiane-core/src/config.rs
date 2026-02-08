use crate::xdg;
use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;

fn default_state_dir() -> PathBuf {
    xdg::data_dir(Some(crate::NAME))
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct GeneralConfig {
    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            state_dir: default_state_dir(),
        }
    }
}

pub fn load<T>(path: impl AsRef<std::path::Path>) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de> + Default,
{
    let file_str = match std::fs::read_to_string(path.as_ref()) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(T::default());
        }
        Err(e) => return Err(e).context("Failed to read configuration file"),
    };

    let parsed = toml::from_str::<T>(&file_str)
        .context("Failed to parse TOML from configuration file")?;

    Ok(parsed)
}
