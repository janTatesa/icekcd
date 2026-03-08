use yanet::Result;

use iced::{Color, Font, color};
use serde::{Deserialize, Deserializer, de};
use serde_inline_default::serde_inline_default;
use std::fs;
use std::str::FromStr;

#[serde_inline_default]
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_font", default)]
    pub font: Font,
    #[serde_inline_default(true)]
    pub show_latest_on_startup: bool,
    #[serde_inline_default(true)]
    pub process_image_by_default: bool,
    #[serde_inline_default(20)]
    pub max_history_size: usize,
    #[serde(default)]
    pub colors: Colors,
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str("").unwrap()
    }
}

impl Default for Colors {
    fn default() -> Self {
        toml::from_str("").unwrap()
    }
}

#[serde_inline_default]
#[derive(Deserialize, Debug, Clone, PartialEq, Copy)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    #[serde_inline_default(color!(0xcba6f7))]
    #[serde(deserialize_with = "deserialize_color")]
    pub primary: Color,
    #[serde_inline_default(color!(0xcdd6f4))]
    #[serde(deserialize_with = "deserialize_color")]
    pub text: Color,
    #[serde_inline_default(color!(0x1e1e2e))]
    #[serde(deserialize_with = "deserialize_color")]
    pub bg: Color,
    #[serde_inline_default(color!(0xf38ba8))]
    #[serde(deserialize_with = "deserialize_color")]
    pub danger: Color,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = dirs::config_dir().unwrap().join("icekcd");
        if !path.exists() {
            fs::create_dir(&path)?;
            fs::write(
                path.join("config.toml"),
                include_str!("./default-config.toml"),
            )?;
            return Ok(toml::from_str("")?);
        }

        Ok(toml::from_str(&fs::read_to_string(
            path.join("config.toml"),
        )?)?)
    }
}

fn deserialize_font<'de, D>(deserializer: D) -> Result<Font, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Font::with_name(String::deserialize(deserializer)?.leak()))
}

fn deserialize_color<'de, D>(deserializer: D) -> Result<Color, D::Error>
where
    D: Deserializer<'de>,
{
    Color::from_str(&String::deserialize(deserializer)?).map_err(de::Error::custom)
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    #[test]
    fn test_config_equality() {
        assert_eq!(
            toml::from_str::<Config>("").unwrap(),
            toml::from_str(include_str!("./default-config.toml")).unwrap()
        )
    }
}
