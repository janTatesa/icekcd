use std::{fmt::Display, str::FromStr, time::Duration};

use isahc::{AsyncBody, AsyncReadResponseExt, Request, Response, config::Configurable};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;
use yanet::{Report, Result, yanet};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Xkcd {
    #[serde(deserialize_with = "de_num_from_value")]
    pub month: u8,
    pub num: u32,
    #[serde(deserialize_with = "de_num_from_value")]
    pub year: u32,
    pub title: String,
    pub alt: String,
    pub img: String,
    #[serde(deserialize_with = "de_num_from_value")]
    pub day: u8,
    #[serde(default, deserialize_with = "de_interactive", rename = "extra_parts")]
    pub is_interactive: bool,
}

fn de_num_from_value<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + TryFrom<u64>,
    T::Err: Display,
    <T as TryFrom<u64>>::Error: Display,
{
    let val = Value::deserialize(deserializer)?;
    match val {
        Value::Number(number) => number
            .as_u64()
            .ok_or(de::Error::custom("Number is not u64"))?
            .try_into()
            .map_err(de::Error::custom),
        Value::String(str) => str.parse().map_err(de::Error::custom),
        _ => Err(de::Error::custom(
            "Value can be either a number or a number as string",
        )),
    }
}

fn de_interactive<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let val = Value::deserialize(deserializer)?;
    match val {
        Value::Bool(bool) => Ok(bool),
        Value::Object(_) => Ok(true),
        _ => Err(de::Error::custom("Value can be either boolean or object")),
    }
}

#[derive(Clone, Copy)]
pub enum Locator {
    Number(u32),
    Latest,
}

impl Xkcd {
    const TIMEOUT: Duration = Duration::from_secs(6);
    pub async fn request(url: &str) -> Result<Response<AsyncBody>> {
        let response =
            isahc::send_async(Request::get(url).timeout(Self::TIMEOUT).body(())?).await?;
        if response.status().is_client_error() || response.status().is_server_error() {
            return Err(yanet!("HTTP response: {}", response.status()));
        }

        Ok(response)
    }

    pub async fn get_latest() -> Result<Self> {
        let text = Self::request("https://xkcd.com/info.0.json")
            .await?
            .text()
            .await?;
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn get(comic: u32) -> Result<Self> {
        let s = &Self::request(&format!("https://xkcd.com/{comic}/info.0.json"))
            .await?
            .text()
            .await?;
        Ok(serde_json::from_str(s)?)
    }

    pub fn parse_locator(locator: &str) -> Option<Locator> {
        if let Ok(locator) = locator.parse() {
            return Some(Locator::Number(locator));
        }

        if locator == "latest" {
            return Some(Locator::Latest);
        }

        let stripped = locator
            .strip_prefix("https://xkcd.com")
            .or(locator.strip_prefix("https://m.xkcd.com"))
            .or(locator.strip_prefix("https://www.explainxkcd.com/wiki/index.php"))
            .or(locator.strip_prefix("https://explainxkcd.com/wiki/index.php"))
            .or(locator.strip_prefix("https://www.explainxkcd.com"))
            .or(locator.strip_prefix("https://explainxkcd.com"))
            .unwrap_or(locator);
        match stripped {
            "/" | "" | "/Main_Page" => Some(Locator::Latest),
            num => {
                if let Some(num) = num.strip_prefix("/")
                    // TODO: use trim_suffix when stabilized
                    && let num = num.strip_suffix("/").unwrap_or(num)
                    && let Some(num) = num.split(":").next()
                    && let Ok(num) = num.parse()
                {
                    Some(Locator::Number(num))
                } else {
                    None
                }
            }
        }
    }
}
