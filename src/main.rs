mod config;
mod event;
mod explanation;
mod history;
mod image;
mod state;
mod update;
mod view;
mod xkcd;

use std::{collections::HashMap, f32, time::Duration};

use clap::Parser;
use iced::{
    Subscription, Task, Theme,
    futures::{join, stream},
    theme::palette::Seed,
    window::{self},
};
use yanet::Result;

use log::error;
use lucide_icons::LUCIDE_FONT_BYTES;

use crate::{
    config::{Colors, Config},
    event::handle_iced_event,
    explanation::{Explanation, Link},
    image::ImageHandles,
    state::State,
    xkcd::{Locator, Xkcd},
};

#[derive(Parser)]
struct Cli {
    /// Xkcd number, url or "latest"
    #[arg(value_parser = parse_cli_locator)]
    xkcd: Option<Locator>,
}

fn parse_cli_locator(locator: &str) -> Result<Locator, &'static str> {
    Xkcd::parse_locator(locator).ok_or("Expected either a number or xkcd url")
}

fn main() -> Result<()> {
    env_logger::init();
    let Cli { xkcd: xkcd_locator } = Cli::parse();
    let icon = Some(window::icon::from_file_data(
        include_bytes!("./icon.png"),
        Some(::image::ImageFormat::Png),
    )?);
    iced::application(
        move || Icekcd::boot(xkcd_locator.clone()),
        Icekcd::update,
        Icekcd::view,
    )
    .theme(Icekcd::theme)
    .title(Icekcd::title)
    .subscription(Icekcd::subscription)
    .scale_factor(Icekcd::scale)
    .window(window::Settings {
        icon,
        ..Default::default()
    })
    .font(LUCIDE_FONT_BYTES)
    .run()?;
    Ok(())
}

type ImageHandlesWrapped = Option<Result<ImageHandles, String>>;
type ExplanationWrapped = Option<Result<Explanation, String>>;

#[allow(clippy::large_enum_variant)]
enum Icekcd {
    Starting(Option<Config>, Option<Locator>),
    InitFailure(String, Option<Config>, Option<Locator>),
    Running(Running),
}

#[derive(Clone, Debug)]
enum ImageKind {
    Xkcd,
    Favorite(Xkcd),
    Explanation(ExplanationKind, usize),
}

#[derive(Clone, Debug)]
enum Message {
    Run(Box<(Xkcd, State, Config, Vec<Message>)>),
    InitError(String),
    Reboot,

    ToggleProcessImage,
    // Xkcd num is necessary in the case of fetching of an image while comic is switched
    ImageFetched(u32, ImageKind, Vec<u8>),
    ImageFetchError(u32, ImageKind, String),
    FetchImage(u32, ImageKind),

    ToggleShowExplanation,
    FetchExplanation(ExplanationKind),
    ExplanationFetched(u32, ExplanationKind, String),
    ExplanationFetchError(u32, ExplanationKind, String),
    LinkClicked(Link),

    OpenExplanationInBrowser,

    ClosePopup,

    DragSplit(f32),
    DragSplitLeft,
    DragSplitRight,

    ToggleBookmark,

    GoToComic(u32),
    GoToNext,
    GoToPrevious,
    GoToLatest,
    GoToRandom,
    GoToBookmark,

    Paste,
    Copy,

    OpenInBrowser,

    ScaleUp,
    ScaleDown,
    ScaleReset,

    HistoryNext,
    HistoryBack,

    LinkHover(Link),
    HoverEnd,

    XkcdFetched(Xkcd),
    LatestXkcdFetched(Xkcd),
    Error(String),

    ReloadState,

    ScrollUp,
    ScrollDown,
    ScrollToStart,
    ScrollToEnd,

    Noop,

    ToggleShowFavorites,
    ToggleFavorite,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
enum ExplanationKind {
    Comic,
    Article,
}

impl ExplanationKind {
    fn id(self) -> &'static str {
        match self {
            ExplanationKind::Comic => "explanation",
            ExplanationKind::Article => "article",
        }
    }
}

const FONT_SIZE: f32 = 16.0;
impl Icekcd {
    fn boot(locator: Option<Locator>) -> (Self, Task<Message>) {
        let config = match Config::load() {
            Ok(config) => config,
            Err(error) => {
                return (
                    Self::InitFailure(error.to_string(), None, locator),
                    Task::none(),
                );
            }
        };

        (
            Self::Starting(Some(config.clone()), locator.clone()),
            Task::future(Self::boot_inner(locator, config.clone())).map(|res| match res {
                Ok(running) => Message::Run(Box::new(running)),
                Err(error) => Message::InitError(error.to_string()),
            }),
        )
    }

    async fn boot_inner(
        locator: Option<Locator>,
        config: Config,
    ) -> Result<(Xkcd, State, Config, Vec<Message>)> {
        let open_xkcd = locator
            .as_ref()
            .is_some_and(|locator| !matches!(locator, Locator::Article(_)))
            || config.show_latest_on_startup;

        let (xkcd, latest_xkcd, article) = match locator {
            Some(Locator::Number(xkcd)) => {
                let (xkcd, latest) = join!(Xkcd::get(xkcd), Xkcd::get_latest());
                (xkcd?, latest?, None)
            }
            Some(Locator::Article(article)) => {
                let xkcd = Xkcd::get_latest().await?;
                (xkcd.clone(), xkcd, Some(article))
            }
            _ => {
                let xkcd = Xkcd::get_latest().await?;
                (xkcd.clone(), xkcd, None)
            }
        };

        let mut state = State::load(xkcd, open_xkcd, config.max_history_size)?;
        if let Some(article) = article {
            state.open_article(article, config.max_history_size)?;
        }

        let mut tasks = vec![
            Message::FetchImage(state.history().current_entry().xkcd.num, ImageKind::Xkcd),
            Message::FetchExplanation(ExplanationKind::Comic),
            Message::FetchExplanation(ExplanationKind::Article),
        ];

        if state.show_favorites() {
            tasks.extend(state.favorites().iter().map(|xkcd| {
                Message::FetchImage(
                    state.history().current_entry().xkcd.num,
                    ImageKind::Favorite(xkcd.clone()),
                )
            }));
        }

        Ok((latest_xkcd, state, config, tasks))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::event::listen().filter_map(handle_iced_event),
            Subscription::run(|| {
                stream::unfold(0, move |i| async move {
                    if i > 0 {
                        _ = smol::Timer::after(Duration::from_mins(15)).await;
                    }

                    let msg = match Xkcd::get_latest().await {
                        Ok(xkcd) => Message::LatestXkcdFetched(xkcd),
                        Err(error) => Message::Error(error.to_string()),
                    };
                    Some((msg, i + 1))
                })
            }),
        ])
    }

    fn theme(&self) -> Theme {
        let Colors {
            primary,
            text,
            bg,
            danger,
        } = match self {
            Icekcd::Starting(config, _) | Icekcd::InitFailure(_, config, _) => {
                config.as_ref().unwrap_or(&Config::default()).colors
            }
            Icekcd::Running(Running { config, .. }) => config.colors,
        };
        Theme::custom(
            "Custom theme",
            Seed {
                background: bg,
                text,
                primary,
                success: primary,
                warning: danger,
                danger,
            },
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match self {
            Icekcd::InitFailure(_, _, locator) => {
                if let Message::Reboot = message {
                    let (app, task) = Self::boot(locator.take());
                    *self = app;
                    return task;
                }
            }
            Icekcd::Running(running) => {
                return running.try_update(message).unwrap_or_else(|err| {
                    running.error = Some(err.to_string());
                    error!("{err}");
                    Task::none()
                });
            }
            Icekcd::Starting(config, locator) => match message {
                Message::InitError(error) => {
                    *self = Icekcd::InitFailure(error, config.clone(), locator.take())
                }

                Message::Run(boxed) => {
                    let (latest_xkcd, state, config, messages) = *boxed;
                    *self = Icekcd::Running(Running {
                        latest_xkcd,
                        state,
                        explanation: None,
                        article: None,
                        favorite_images: HashMap::new(),
                        hovered_link: None,
                        error: None,
                        config,
                        image_handles: None,
                        save_on_fetch: false,
                    });

                    return Task::batch(messages.into_iter().map(Task::done));
                }

                _ => {}
            },
        }

        Task::none()
    }

    fn title(&self) -> String {
        match self {
            Icekcd::Starting(_, _) | Icekcd::InitFailure(_, _, _) => "Icekcd".to_string(),
            Icekcd::Running(running) => {
                format!("Icekcd - {}: {}", running.xkcd().num, running.xkcd().title)
            }
        }
    }

    fn scale(&self) -> f32 {
        match self {
            Icekcd::Starting(_, _) | Icekcd::InitFailure(_, _, _) => 1.0,
            Icekcd::Running(running) => running.state.scale(),
        }
    }
}

struct Running {
    latest_xkcd: Xkcd,
    state: State,
    explanation: ExplanationWrapped,
    article: ExplanationWrapped,
    favorite_images: HashMap<u32, ImageHandlesWrapped>,
    hovered_link: Option<String>,
    error: Option<String>,
    config: Config,
    image_handles: ImageHandlesWrapped,
    save_on_fetch: bool,
}
