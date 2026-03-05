use std::{f32, iter};

use color_eyre::Result;
use iced::{
    Task,
    clipboard::{self, Content, Kind},
    widget::operation::{AbsoluteOffset, scroll_by, scroll_to},
};

use isahc::AsyncReadResponseExt;

use crate::{
    ExplanationKind, FONT_SIZE, Image, ImageHandlesWrapped, Message, Running,
    explanation::{Explanation, Link},
    history::HistoryEntry,
    image::process_image,
    xkcd::{Locator, Xkcd},
};

impl Running {
    pub fn try_update(&mut self, message: Message) -> Result<Task<Message>> {
        match message {
            Message::ToggleProcessImage => self
                .state
                .toggle_processing(self.config.process_image_by_default)?,
            Message::ToggleShowExplanation => {
                self.hovered_link = None;
                self.state.toggle_show_explanation()?
            }
            Message::ImageFetched(num, image, bytes) if self.xkcd().num == num => {
                let fg = self.config.colors.text;
                let bg = self.config.colors.bg;
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
                if let Some(image) = self.image_handles(image.clone()) {
                    *image = None;
                }
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
                match kind {
                    ExplanationKind::Comic => self.explanation = None,
                    ExplanationKind::Article => self.article = None,
                };

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
            Message::Reboot | Message::Run(_) | Message::InitError(_) => {}
        }

        Ok(Task::none())
    }

    pub fn current_entry(&self) -> &HistoryEntry {
        self.state.history().current_entry()
    }

    pub fn xkcd(&self) -> &Xkcd {
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
}
