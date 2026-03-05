mod config;
mod event;
mod explanation;
mod history;
mod image;
mod state;
mod view;
mod xkcd;

use std::{collections::HashMap, f32, iter, time::Duration};

use clap::Parser;
use color_eyre::{Result, eyre::OptionExt};
use iced::{
    Subscription, Task, Theme,
    clipboard::{self, Content, Kind},
    futures::{join, stream},
    theme::{Palette, palette},
    widget::operation::{AbsoluteOffset, scroll_by, scroll_to},
    window::{self},
};

use isahc::AsyncReadResponseExt;
use log::error;
use lucide_icons::LUCIDE_FONT_BYTES;

use crate::{
    config::{Colors, Config},
    event::handle_iced_event,
    explanation::{Explanation, Link},
    history::HistoryEntry,
    image::{ImageHandles, process_image},
    state::State,
    xkcd::{Locator, Xkcd},
};

#[derive(Parser)]
struct Cli {
    /// Xkcd number, url or "latest"
    #[arg(value_parser = parse_cli_locator)]
    xkcd: Option<Locator>,
}

fn parse_cli_locator(locator: &str) -> Result<Locator> {
    Xkcd::parse_locator(locator).ok_or_eyre("Expected either a number or xkcd url")
}

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::init();
    let Cli { xkcd: xkcd_locator } = Cli::parse();
    let icon = Some(window::icon::from_file_data(
        include_bytes!("./icon.png"),
        Some(::image::ImageFormat::Png),
    )?);
    iced::application(
        move || Icekcd::boot(xkcd_locator),
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
    InitFailure(String, Option<Config>, Option<Locator>),
    Running(Running),
}

#[derive(Clone, Debug)]
enum Image {
    Xkcd,
    Favorite(Xkcd),
    Explanation(ExplanationKind, usize),
}

#[derive(Clone, Debug)]
enum Message {
    Reboot,

    ToggleProcessImage,
    // Xkcd num is necessary in the case of fetching of an image while comic is switched
    ImageFetched(u32, Image, Vec<u8>),
    ImageFetchError(u32, Image, String),
    FetchImage(u32, Image),

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
        let (xkcd, latest_xkcd) = match locator {
            Some(Locator::Number(xkcd)) => {
                let future = async { join!(Xkcd::get(xkcd), Xkcd::get_latest()) };
                match smol::block_on(future) {
                    (Err(error), _) | (_, Err(error)) => {
                        return (
                            Self::InitFailure(error.to_string(), Some(config), locator),
                            Task::none(),
                        );
                    }
                    (Ok(xkcd), Ok(latest)) => (xkcd, latest),
                }
            }
            _ => {
                let xkcd = match smol::block_on(Xkcd::get_latest()) {
                    Ok(xkcd) => xkcd,
                    Err(error) => {
                        return (
                            Self::InitFailure(error.to_string(), Some(config), locator),
                            Task::none(),
                        );
                    }
                };
                (xkcd.clone(), xkcd)
            }
        };

        let open_xkcd = locator.is_some() || config.show_latest_on_startup;
        let state = match State::load(xkcd, open_xkcd, config.max_history_size) {
            Ok(state) => state,
            Err(_) => todo!(),
        };

        let mut tasks = Vec::from(
            [
                Message::FetchImage(state.history().current_entry().xkcd.num, Image::Xkcd),
                Message::FetchExplanation(ExplanationKind::Comic),
                Message::FetchExplanation(ExplanationKind::Article),
            ]
            .map(Task::done),
        );

        if state.show_favorites() {
            tasks.extend(state.favorites().iter().map(|xkcd| {
                Task::done(Message::FetchImage(
                    state.history().current_entry().xkcd.num,
                    Image::Favorite(xkcd.clone()),
                ))
            }));
        }
        let batch = Task::batch(tasks);
        (
            Self::Running(Running {
                latest_xkcd: latest_xkcd.clone(),
                state: state.clone(),
                explanation: None,
                hovered_link: None,
                error: None,
                config: config.clone(),
                image_handles: None,
                article: None,
                favorite_images: HashMap::new(),
            }),
            batch,
        )
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
            Icekcd::InitFailure(_, config, _) => {
                config.as_ref().unwrap_or(&Config::default()).colors
            }
            Icekcd::Running(Running { config, .. }) => config.colors,
        };
        Theme::custom(
            "Custom theme",
            iced::theme::Palette {
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
                    let (app, task) = Self::boot(*locator);
                    *self = app;
                    task
                } else {
                    Task::none()
                }
            }
            Icekcd::Running(running) => running.try_update(message).unwrap_or_else(|err| {
                running.error = Some(err.to_string());
                error!("{err}");
                Task::none()
            }),
        }
    }

    fn title(&self) -> String {
        match self {
            Icekcd::InitFailure(_, _, _) => "Icekcd".to_string(),
            Icekcd::Running(running) => {
                format!("Icekcd - {}: {}", running.xkcd().num, running.xkcd().title)
            }
        }
    }

    fn scale(&self) -> f32 {
        match self {
            Icekcd::InitFailure(_, _, _) => 1.0,
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
}

impl Running {
    fn try_update(&mut self, message: Message) -> Result<Task<Message>> {
        match message {
            Message::ToggleProcessImage => self
                .state
                .toggle_processing(self.config.process_image_by_default)?,
            Message::ToggleShowExplanation => {
                self.hovered_link = None;
                self.state.toggle_show_explanation()?
            }
            Message::ImageFetched(num, image, bytes) if self.xkcd().num == num => {
                let fg = self.palette().text;
                let bg = self.palette().background;
                if let Some(image_handles) = self.image_handles(image) {
                    let handles = match ::image::load_from_memory(&bytes) {
                        Ok(image) => {
                            let handles = process_image(image, fg, bg);
                            Ok(handles)
                        }
                        Err(report) => Err(report.to_string()),
                    };

                    *image_handles = Some(handles);
                }
            }
            Message::ImageFetched(_, _, _) => {}
            Message::ToggleBookmark => self.state.toggle_bookmark(self.xkcd().num)?,
            Message::GoToComic(comic)
                if (1..=self.latest_xkcd.num).contains(&comic) && self.xkcd().num != comic =>
            {
                return Ok(self.open_comic(comic));
            }
            Message::GoToComic(_) => {}
            Message::GoToLatest => {
                self.state
                    .open_xkcd(self.latest_xkcd.clone(), self.config.max_history_size)?;
                self.on_comic_switch()?;
                return Ok(Task::batch(
                    [
                        Message::FetchImage(self.xkcd().num, Image::Xkcd),
                        Message::FetchExplanation(ExplanationKind::Comic),
                    ]
                    .map(Task::done),
                ));
            }
            Message::GoToRandom => {
                let random = rand::random_range(1..=self.latest_xkcd.num);
                return Ok(self.open_comic(random));
            }
            Message::GoToNext if self.xkcd().num < self.latest_xkcd.num => {
                return Ok(self.open_comic(self.xkcd().num + 1));
            }
            Message::GoToNext => {}
            Message::GoToPrevious => {
                return Ok(self.open_comic(self.xkcd().num - 1));
            }
            Message::ReloadState => self.state.reload()?,
            Message::Paste => {
                return Ok(clipboard::read(Kind::Text).map(|pasted| {
                    let Content::Text(pasted) = (match &pasted {
                        Ok(pasted) => &**pasted,
                        Err(error) => {
                            return Message::Error(
                                match error {
                                    clipboard::Error::ClipboardUnavailable => {
                                        "Clipboard unavailable"
                                    }
                                    clipboard::Error::ClipboardOccupied => "Clipboard occupied",
                                    clipboard::Error::ContentNotAvailable => {
                                        "Content not available"
                                    }
                                    clipboard::Error::ConversionFailure => "Conversion failure",
                                    clipboard::Error::Unknown { description } => {
                                        description.as_str()
                                    }
                                }
                                .to_string(),
                            );
                        }
                    }) else {
                        return Message::Noop;
                    };

                    match Xkcd::parse_locator(pasted) {
                        Some(Locator::Latest) => Message::GoToLatest,
                        Some(Locator::Number(num)) => Message::GoToComic(num),
                        None => Message::Noop,
                    }
                }));
            }
            Message::Noop => {}
            Message::Copy => {
                let num = self.xkcd().num;
                return Ok(
                    clipboard::write(format!("https://xkcd.com/{num}")).map(
                        |result| match result {
                            Ok(_) => Message::Noop,
                            Err(error) => Message::Error(
                                match &error {
                                    clipboard::Error::ClipboardUnavailable => {
                                        "Clipboard unavailable"
                                    }
                                    clipboard::Error::ClipboardOccupied => "Clipboard occupied",
                                    clipboard::Error::ContentNotAvailable => {
                                        "Content not available"
                                    }
                                    clipboard::Error::ConversionFailure => "Conversion failure",
                                    clipboard::Error::Unknown { description } => {
                                        description.as_str()
                                    }
                                }
                                .to_string(),
                            ),
                        },
                    ),
                );
            }
            Message::ExplanationFetched(num, kind, explanation) if num == self.xkcd().num => {
                let explanation = Explanation::new(&explanation, kind).ok_or_else(|| {
                    "Couldn't parse explanation, please submit a github issue".to_string()
                });
                let scroll_to = scroll_to(kind.id(), AbsoluteOffset { x: 0.0, y: 0.0 });
                let task = match &explanation {
                    Ok(explanation) => Task::batch(iter::once(scroll_to).chain(
                        (0..explanation.images.len()).map(|idx| {
                            Task::done(Message::FetchImage(num, Image::Explanation(kind, idx)))
                        }),
                    )),
                    Err(_) => Task::none(),
                };

                match kind {
                    ExplanationKind::Comic => self.explanation = Some(explanation),
                    ExplanationKind::Article => self.article = Some(explanation),
                }
                return Ok(task);
            }
            Message::ExplanationFetched(_, _, _) => {}
            Message::DragSplit(split) => self.state.drag_split(split)?,
            Message::OpenExplanationInBrowser => {
                open::that(format!("https://explainxkcd.com/{}", self.xkcd().num))?;
            }
            Message::LinkClicked(Link::Xkcd(xkcd)) => return Ok(self.open_comic(xkcd)),
            Message::LinkClicked(Link::SelfLink) => {
                if self.current_entry().article.is_some() {
                    self.state.close_article(self.config.max_history_size)?
                }

                return Ok(scroll_to(
                    ExplanationKind::Comic.id(),
                    AbsoluteOffset { x: 0.0, y: 0.0 },
                ));
            }
            Message::LinkClicked(Link::Url(url)) => {
                open::that(&url)?;
                self.state.open_url(url)?;
            }
            Message::LinkClicked(Link::ExplainXkcdUrl(url)) => {
                self.state.open_article(url, self.config.max_history_size)?;
                return Ok(Task::done(Message::FetchExplanation(
                    ExplanationKind::Article,
                )));
            }
            Message::DragSplitLeft => self.state.drag_split(match self.state.split() {
                ..0.1 => 0.0,
                split => split - 0.1,
            })?,
            Message::DragSplitRight => self.state.drag_split(match self.state.split() {
                0.9.. => 1.0,
                split => split + 0.1,
            })?,
            Message::ScaleUp => self.state.set_scale(match self.state.scale() {
                3.0.. => return Ok(Task::none()),
                scale => scale + 0.1,
            })?,
            Message::ScaleDown => self.state.set_scale(match self.state.scale() {
                ..0.5 => return Ok(Task::none()),
                scale => scale - 0.1,
            })?,
            Message::ScaleReset => self.state.set_scale(1.0)?,
            Message::HistoryNext => {
                let (changed, res) = self.state.history_forward();
                let res2 = if changed {
                    self.on_comic_switch()
                } else {
                    Ok(())
                };

                let msgs = [
                    Message::FetchImage(self.xkcd().num, Image::Xkcd),
                    Message::FetchExplanation(ExplanationKind::Comic),
                    Message::FetchExplanation(ExplanationKind::Article),
                ]
                .map(Task::done);
                let mut tasks = Vec::from(msgs);

                if let Err(error) = res {
                    tasks.push(Task::done(Message::Error(error.to_string())));
                }

                if let Err(error) = res2 {
                    tasks.push(Task::done(Message::Error(error.to_string())));
                }

                return Ok(Task::batch(tasks));
            }
            Message::HistoryBack => {
                let (changed, res) = self.state.history_backward();
                let res2 = if changed {
                    self.on_comic_switch()
                } else {
                    Ok(())
                };

                let msgs = [
                    Message::FetchImage(self.xkcd().num, Image::Xkcd),
                    Message::FetchExplanation(ExplanationKind::Comic),
                    Message::FetchExplanation(ExplanationKind::Article),
                ]
                .map(Task::done);
                let mut tasks = Vec::from(msgs);

                if let Err(error) = res {
                    tasks.push(Task::done(Message::Error(error.to_string())));
                }

                if let Err(error) = res2 {
                    tasks.push(Task::done(Message::Error(error.to_string())));
                }

                return Ok(Task::batch(tasks));
            }
            Message::GoToBookmark => {
                if let Some(bookmark) = self.state.bookmarked()
                    && bookmark != self.xkcd().num
                {
                    return Ok(self.open_comic(self.xkcd().num));
                }
            }
            Message::LinkHover(Link::Url(url) | Link::ExplainXkcdUrl(url)) => {
                self.hovered_link = Some(url)
            }
            Message::LinkHover(_) => {}
            Message::HoverEnd => self.hovered_link = None,
            Message::ImageFetchError(num, image, error) if self.xkcd().num == num => {
                if let Some(handle) = self.image_handles(image) {
                    *handle = Some(Err(error));
                }
            }
            Message::ImageFetchError(_, _, _) => {}
            Message::ExplanationFetchError(num, kind, error) if self.xkcd().num == num => {
                match kind {
                    ExplanationKind::Comic => self.explanation = Some(Err(error)),
                    ExplanationKind::Article => self.article = Some(Err(error)),
                }
            }
            Message::ExplanationFetchError(_, _, _) => {}
            Message::XkcdFetched(xkcd) => {
                self.state.open_xkcd(xkcd, self.config.max_history_size)?;
                self.on_comic_switch()?;
                return Ok(Task::batch([
                    Task::done(Message::FetchImage(self.xkcd().num, Image::Xkcd)),
                    Task::done(Message::FetchExplanation(ExplanationKind::Comic)),
                ]));
            }
            Message::Error(err) => self.error = Some(err),
            Message::LatestXkcdFetched(xkcd) => self.latest_xkcd = xkcd,
            Message::ScrollUp => {
                let offset = AbsoluteOffset {
                    x: 0.0,
                    y: -FONT_SIZE,
                };
                return Ok(scroll_by(self.scrollable_id(), offset));
            }
            Message::ScrollDown => {
                let offset = AbsoluteOffset {
                    x: 0.0,
                    y: FONT_SIZE,
                };
                return Ok(scroll_by(self.scrollable_id(), offset));
            }
            Message::OpenInBrowser => open::that(format!("https://xkcd.com/{}", self.xkcd().num))?,
            Message::FetchImage(xkcd, image) => {
                let url = match &image {
                    Image::Xkcd => self.xkcd().img.clone(),
                    Image::Favorite(xkcd) => xkcd.img.clone(),
                    Image::Explanation(explanation_kind, idx) => {
                        let Some(Ok(explanation)) = (match explanation_kind {
                            ExplanationKind::Comic => &self.explanation,
                            ExplanationKind::Article => &self.article,
                        }) else {
                            return Ok(Task::none());
                        };

                        explanation.images[*idx].1.clone()
                    }
                };

                let image2 = image.clone();
                return Ok(Task::future(async move {
                    let bytes = Xkcd::request(&url).await?.bytes().await?;
                    Ok(Message::ImageFetched(xkcd, image, bytes))
                })
                .map(move |res: Result<_>| {
                    res.unwrap_or_else(|err| {
                        Message::ImageFetchError(xkcd, image2.clone(), err.to_string())
                    })
                }));
            }
            Message::FetchExplanation(kind) => {
                let num = self.xkcd().num;
                let explanation_url = match kind {
                    ExplanationKind::Comic => format!("https://explainxkcd.com/{num}"),
                    ExplanationKind::Article => {
                        if let Some(url) = &self.current_entry().article {
                            url.clone()
                        } else {
                            return Ok(Task::none());
                        }
                    }
                };
                return Ok(Task::future(async move {
                    let text = Xkcd::request(&explanation_url).await?.text().await?;
                    Ok(Message::ExplanationFetched(num, kind, text))
                })
                .map(move |res: Result<_>| {
                    res.unwrap_or_else(|err| {
                        Message::ExplanationFetchError(num, kind, err.to_string())
                    })
                }));
            }
            Message::ClosePopup => {
                if self.state.show_favorites() {
                    self.state.toggle_show_favorites()?
                } else {
                    self.state.close_article(self.config.max_history_size)?
                }
            }
            Message::ToggleShowFavorites => {
                self.state.toggle_show_favorites()?;
                if self.state.show_favorites() {
                    return Ok(Task::batch(self.state.favorites().iter().map(|xkcd| {
                        if !self.favorite_images.contains_key(&xkcd.num) {
                            Task::done(Message::FetchImage(
                                self.xkcd().num,
                                Image::Favorite(xkcd.clone()),
                            ))
                        } else {
                            Task::none()
                        }
                    })));
                }
            }
            Message::ToggleFavorite => self.state.toggle_favorite()?,
            Message::ScrollToStart => {
                return Ok(scroll_to(
                    self.scrollable_id(),
                    AbsoluteOffset {
                        x: None,
                        y: Some(0.0),
                    },
                ));
            }
            Message::ScrollToEnd => {
                return Ok(scroll_to(
                    self.scrollable_id(),
                    AbsoluteOffset {
                        x: None,
                        y: Some(f32::MAX),
                    },
                ));
            }
            Message::Reboot => todo!(),
        }

        Ok(Task::none())
    }

    fn current_entry(&self) -> &HistoryEntry {
        self.state.history().current_entry()
    }

    fn xkcd(&self) -> &Xkcd {
        &self.current_entry().xkcd
    }

    fn open_comic(&mut self, num: u32) -> Task<Message> {
        self.error = None;
        Task::future(async move {
            match Xkcd::get(num).await {
                Ok(xkcd) => Message::XkcdFetched(xkcd),
                Err(error) => Message::Error(error.to_string()),
            }
        })
    }

    fn on_comic_switch(&mut self) -> Result<()> {
        self.error = None;
        self.explanation = None;
        self.article = None;
        self.image_handles = None;
        self.hovered_link = None;
        if self.state.show_favorites() {
            self.state.toggle_show_favorites()?
        }

        Ok(())
    }

    fn explanation_kind(&self) -> ExplanationKind {
        if self.current_entry().article.is_some() {
            ExplanationKind::Article
        } else {
            ExplanationKind::Comic
        }
    }

    fn scrollable_id(&self) -> &'static str {
        if self.state.show_favorites() {
            "favorites"
        } else {
            self.explanation_kind().id()
        }
    }

    fn image_handles(&mut self, image: Image) -> Option<&mut ImageHandlesWrapped> {
        Some(match image {
            Image::Xkcd => &mut self.image_handles,
            Image::Favorite(xkcd) => self.favorite_images.entry(xkcd.num).or_default(),
            Image::Explanation(kind, idx) => {
                let explanation = match kind {
                    ExplanationKind::Comic => self.explanation.as_mut(),
                    ExplanationKind::Article => self.article.as_mut(),
                };
                &mut explanation?.as_mut().ok()?.images[idx].0
            }
        })
    }

    fn palette(&self) -> Palette {
        let Colors {
            primary,
            text,
            bg,
            danger,
        } = self.config.colors;
        Palette {
            background: bg,
            text,
            primary,
            success: primary,
            warning: danger,
            danger,
        }
    }

    fn extended_palette(&self) -> palette::Extended {
        palette::Extended::generate(self.palette())
    }
}
