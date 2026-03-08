use crate::config::{Colors, Config};
use crate::explanation::{Description, ExplanationElement, Heading, Modifiers, Span};
use crate::state::Viewable;
use crate::{ExplanationKind, FONT_SIZE, Icekcd, ImageHandlesWrapped, ImageKind, Message, Running};
use iced::theme::{Palette, palette};
use iced::widget::Row;
use iced::{
    Alignment, Color, Element, Font, Length, Shadow, Theme, Vector, border,
    font::{self, Weight},
    never, padding,
    widget::{
        self, Button, Column, button, column, container,
        image::viewer,
        row,
        rule::{self, FillMode},
        space, span, stack,
        table::{self, Table},
        text,
    },
};

use iced_selection::{rich_text, text::Rich};
use iced_split::vertical_split;
use lucide_icons::Icon;
use std::array;
use std::iter::Peekable;
use std::ops::Not;

use crate::explanation::Link;
const SHADOW: Shadow = Shadow {
    color: Color::BLACK.scale_alpha(0.5),
    offset: Vector::ZERO,
    blur_radius: 8.0,
};

const SPACING: f32 = 5.0;
const BORDER_RADIUS: f32 = 2.0;
const BORDER_WIDTH: f32 = 2.0;
const MAX_IMG_SCALE: f32 = 15.0;
const SMOL_SIZE: f32 = 12.0;
const MAX_EXPLANATION_WIDTH: f32 = 700.0;

impl Running {
    pub fn view(&self) -> Element<'_, Message> {
        let xkcd = self.xkcd();
        let palette = self.extended_palette();
        let history_back = Self::button(Icon::ArrowLeft, button::primary).on_press_maybe(
            self.state
                .history()
                .can_go_backward()
                .then_some(Message::HistoryBack),
        );

        let history_forward = Self::button(Icon::ArrowRight, button::primary).on_press_maybe(
            self.state
                .history()
                .can_go_forward()
                .then_some(Message::HistoryNext),
        );

        let font = self.config.font;
        let mut title_font = self.config.font;
        title_font.weight = Weight::Bold;
        let title = rich_text![
            span(format!("{}: ", xkcd.num))
                .color(palette.secondary.base.color)
                .font(title_font),
            span(&xkcd.title).font(title_font).link(()),
            span(" (").color(palette.secondary.base.color).font(font),
            span(format!("{}-{:02}-{:02}", xkcd.year, xkcd.month, xkcd.day))
                .color(palette.primary.base.color)
                .font(font),
            span(")").color(palette.secondary.base.color).font(font)
        ]
        .on_link_click(|_: ()| Message::OpenInBrowser);
        let title = container(title).center_x(Length::Fill);

        let is_bookmarked = self.state.bookmarked().is_some_and(|num| num == xkcd.num);

        let toggle_processing = Self::button(
            Icon::WandSparkles,
            if self.processing_enabled(xkcd.num) {
                button::primary
            } else {
                button::secondary
            },
        )
        .on_press(Message::ToggleProcessImage);

        let toggle_explanation = self.state.show_explanation().not().then(|| {
            Self::button(Icon::CircleQuestionMark, button::primary)
                .on_press(Message::ToggleShowExplanation)
        });

        let top_right_buttons = row![toggle_processing, toggle_explanation,].spacing(SPACING);
        let top_right_buttons = container(top_right_buttons).align_right(Length::Fill);
        let top_left_buttons = row![history_back, history_forward]
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .spacing(5);
        let title = stack![top_left_buttons, title.padding(SPACING), top_right_buttons];
        let mut contains_color_notice = None;
        let image: Element<'_, _> = self.fetchable(
            self.image_handles.as_ref(),
            "image",
            Message::FetchImage(self.xkcd().num, ImageKind::Xkcd),
            Length::Fill,
            Length::Fill,
            |handles| {
                if handles.contains_color() && self.processing_enabled(xkcd.num) {
                    contains_color_notice = Some(
                        text("This image contains color. Toggle off processing to view it")
                            .font(font),
                    )
                }

                viewer(handles.get(self.processing_enabled(xkcd.num)))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .min_scale(1.0)
                    .max_scale(MAX_IMG_SCALE)
                    .into()
            },
        );

        let bottom_buttons = row![
            Self::button(Icon::ChevronFirst, button::primary)
                .on_press_maybe(self.xkcd().num.gt(&1).then_some(Message::GoToComic(1))),
            Self::button(Icon::ChevronLeft, button::primary)
                .on_press_maybe(self.xkcd().num.gt(&1).then_some(Message::GoToPrevious)),
            Self::button(Icon::Dice3, button::primary).on_press(Message::GoToRandom),
            Self::button(
                Icon::ChevronRight,
                if self.xkcd().num + 1 == self.latest_xkcd.num
                    && self.state.latest_xkcd_viewed() < self.latest_xkcd.num
                {
                    button::danger
                } else {
                    button::primary
                }
            )
            .on_press_maybe(if self.xkcd().num < self.latest_xkcd.num {
                Some(Message::GoToComic(self.xkcd().num + 1))
            } else {
                None
            }),
            Self::button(
                Icon::ChevronLast,
                if self.state.latest_xkcd_viewed() < self.latest_xkcd.num {
                    button::danger
                } else {
                    button::primary
                }
            )
            .on_press_maybe(if self.xkcd().num < self.latest_xkcd.num {
                Some(Message::GoToLatest)
            } else {
                None
            }),
        ]
        .spacing(SPACING);
        let bottom_buttons = container(bottom_buttons).center_x(Length::Fill);
        let on_press = self.state.bookmarked().map(|_| Message::GoToBookmark);
        let go_to_bookmark = Self::button(Icon::Bookmark, button::primary).on_press_maybe(on_press);
        let toggle_bookmark = if is_bookmarked {
            Self::button(Icon::BookmarkMinus, button::primary)
        } else {
            Self::button(Icon::BookmarkPlus, button::secondary)
        }
        .on_press(Message::ToggleBookmark);
        let bottom_left_buttons = container(row![go_to_bookmark, toggle_bookmark].spacing(SPACING))
            .align_left(Length::Fill);

        let togle_show_favorites =
            Self::button(Icon::Heart, button::primary).on_press(Message::ToggleShowFavorites);
        let toggle_favorite = if self.state.favorites().contains(self.xkcd()) {
            Self::button(Icon::HeartMinus, button::primary)
        } else {
            Self::button(Icon::HeartPlus, button::secondary)
        }
        .on_press(Message::ToggleFavorite);
        let bottom_right_buttons =
            container(row![togle_show_favorites, toggle_favorite].spacing(SPACING))
                .align_right(Length::Fill);
        let error = self
            .error
            .as_ref()
            .map(|err| text(err).style(text::danger).font(font));
        let alt = container(iced_selection::text(&self.xkcd().alt).center().font(font))
            .center_x(Length::Fill);
        let interactive_notice = self.xkcd().is_interactive.then_some(
            text("This comic is interactive. Press ctrl-o to open it in browser").font(font),
        );

        let xkcd_view = container(
            column![
                title,
                image,
                alt,
                stack![bottom_left_buttons, bottom_buttons, bottom_right_buttons],
                error,
                interactive_notice,
                contains_color_notice
            ]
            .spacing(SPACING),
        );

        let main_view: Element<'_, _> = if self.state.show_explanation() {
            let content = vertical_split(
                xkcd_view,
                self.explanation_view(
                    ExplanationKind::Comic,
                    format!("https://explainxkcd.com/{}", self.xkcd().num),
                    Message::ToggleShowExplanation,
                ),
                self.state.split(),
                Message::DragSplit,
            );
            container(content).padding(SPACING).into()
        } else {
            xkcd_view.padding(SPACING).into()
        };

        let article = self.current_entry().article.as_ref().map(|article| {
            self.explanation_view(
                ExplanationKind::Article,
                article.clone(),
                Message::ClosePopup,
            )
        });

        let favorites = self.state.show_favorites().then(|| {
            let favorites = Column::from_iter(
                OptionArrayChunks::<_, 3>(self.state.favorites().iter().peekable()).map(|xkcd| {
                    Row::from_iter(xkcd.map(|xkcd| {
                        let Some(xkcd) = xkcd else {
                            return space().width(Length::FillPortion(1)).into();
                        };

                        let title = container(
                            rich_text![
                                span(format!("{}: ", xkcd.num))
                                    .color(palette.secondary.base.color)
                                    .font(font),
                                span(&xkcd.title).font(title_font)
                            ]
                            .on_link_click(never),
                        )
                        .center_x(Length::Fill);
                        let content = self.fetchable(
                            self.favorite_images.get(&xkcd.num).and_then(Option::as_ref),
                            "image",
                            Message::FetchImage(self.xkcd().num, ImageKind::Favorite(xkcd.clone())),
                            Length::FillPortion(1),
                            Length::Fixed(SPACING * 100.0),
                            |img| {
                                container(
                                    widget::image(img.get(self.processing_enabled(xkcd.num)))
                                        .width(Length::Fill),
                                )
                                .center(Length::Fill)
                                .into()
                            },
                        );
                        button(column![title, content].spacing(SPACING))
                            .padding(SPACING)
                            .style(|theme, status| button::Style {
                                text_color: theme.palette().text,
                                background: Some(theme.palette().background.into()),
                                border: border::color(match status {
                                    button::Status::Active => {
                                        theme.extended_palette().secondary.base.color
                                    }
                                    button::Status::Hovered => theme.palette().primary,
                                    button::Status::Pressed => {
                                        theme.extended_palette().primary.weak.color
                                    }
                                    button::Status::Disabled => unreachable!(),
                                })
                                .width(BORDER_WIDTH)
                                .rounded(BORDER_RADIUS),
                                shadow: SHADOW,
                                snap: true,
                            })
                            .on_press(Message::GoToComic(xkcd.num))
                            .height(400.0)
                            .into()
                    }))
                    .spacing(SPACING)
                    .into()
                }),
            )
            .spacing(SPACING);

            const FAVORITES_WIDTH: f32 = 900.0;
            container(
                container(
                    column![
                        stack![
                            container(text("Favorites").font(title_font))
                                .center_x(Length::Fill)
                                .padding(SPACING),
                            container(
                                Self::button(Icon::CircleX, button::primary)
                                    .on_press(Message::ToggleShowFavorites)
                            )
                            .align_right(Length::Fill)
                        ],
                        widget::scrollable(favorites)
                            .id("favorites")
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .auto_scroll(true),
                    ]
                    .spacing(SPACING),
                )
                .height(Length::Fill)
                .width(FAVORITES_WIDTH)
                .style(|theme| container::Style {
                    text_color: None,
                    background: Some(theme.palette().background.into()),
                    border: border::color(theme.extended_palette().secondary.base.color)
                        .width(BORDER_WIDTH)
                        .rounded(BORDER_RADIUS),
                    shadow: SHADOW,
                    snap: true,
                })
                .padding(SPACING),
            )
            .center_x(Length::Fill)
            .height(Length::Fill)
            .padding(padding::vertical(SPACING))
        });
        let dimmer = (article.is_some() || favorites.is_some()).then_some(
            container(space())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Color::BLACK.scale_alpha(0.5).into()),
                    ..Default::default()
                }),
        );

        stack![main_view, dimmer, article, favorites].into()
    }

    fn explanation_view(
        &self,
        kind: ExplanationKind,
        url: String,
        close_msg: Message,
    ) -> Element<'_, Message> {
        let font = self.config.font;
        let explanation = self.fetchable(
            match kind {
                ExplanationKind::Comic => self.explanation.as_ref(),
                ExplanationKind::Article => self.article.as_ref(),
            },
            kind.id(),
            Message::FetchExplanation(ExplanationKind::Comic),
            Length::Fill,
            Length::Fill,
            |explanation| {
                if explanation.elements().is_empty() {
                    return container(
                        row![
                            Self::button(Icon::RefreshCw, button::primary)
                                .on_press(Message::FetchExplanation(kind)),
                            text("This comic doesn't have an explanation yet").font(font)
                        ]
                        .align_y(Alignment::Center)
                        .spacing(SPACING),
                    )
                    .center(Length::Fill)
                    .into();
                }

                let content = self.explanation_inner(
                    explanation.elements(),
                    kind,
                    Modifiers::default(),
                    &explanation.images,
                );
                let explanation = container(content).center_x(Length::Fill);
                widget::scrollable(explanation)
                    .id(kind.id())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .auto_scroll(true)
                    .into()
            },
        );

        let displayed_url = match kind {
            ExplanationKind::Comic => url.clone(),
            ExplanationKind::Article => url
                .strip_prefix("https://explainxkcd.com/wiki/index.php/")
                .unwrap_or(&url)
                .to_string(),
        };
        let url = rich_text![
            span(displayed_url)
                .font(Font {
                    weight: Weight::Bold,
                    ..self.config.font
                })
                .link(Link::Url(url))
                .color(self.palette().primary)
        ]
        .on_link_click(Message::LinkClicked);
        let close_button = Self::button(Icon::CircleX, button::primary).on_press(close_msg);
        let title_bar = stack![
            container(url).center_x(Length::Fill).padding(SPACING),
            container(close_button).align_right(Length::Fill)
        ];

        let contains_unknown_notice = match &self.explanation {
            Some(Ok(explanation)) if explanation.contains_unknown => Some(text("This explanation contains elements which icekcd couldn't parse, pwease submit an issue").style(text::danger).font(font)),
            _ => None,
        };

        let explanation_view =
            column![title_bar, contains_unknown_notice, explanation].spacing(SPACING);
        let hovered_link = self
            .hovered_link
            .as_deref()
            .filter(|_| self.current_entry().article.is_none() || kind == ExplanationKind::Article)
            .map(|link| {
                let content = container(link)
                    .style(|theme: &Theme| container::Style {
                        text_color: Some(theme.palette().primary),
                        border: border::rounded(BORDER_RADIUS)
                            .width(BORDER_WIDTH)
                            .color(theme.extended_palette().secondary.base.color),
                        shadow: SHADOW,
                        background: Some(theme.palette().background.into()),
                        ..Default::default()
                    })
                    .padding(SPACING);
                container(content).align_bottom(Length::Fill)
            });

        let content = stack![explanation_view, hovered_link].into();
        match kind {
            ExplanationKind::Comic => content,
            ExplanationKind::Article => container(
                container(content)
                    .style(|theme| container::Style {
                        text_color: None,
                        background: Some(theme.palette().background.into()),
                        border: border::color(theme.extended_palette().secondary.base.color)
                            .width(BORDER_WIDTH)
                            .rounded(BORDER_RADIUS),
                        shadow: SHADOW,
                        snap: true,
                    })
                    .padding(SPACING)
                    .width(Length::Fixed(MAX_EXPLANATION_WIDTH))
                    .height(Length::Fill),
            )
            .center_x(Length::Fill)
            .padding(padding::vertical(SPACING))
            .into(),
        }
    }

    fn explanation_inner<'a>(
        &'a self,
        content: impl IntoIterator<Item = &'a ExplanationElement<'a>>,
        kind: ExplanationKind,
        default_modifiers: Modifiers,
        images: &'a [(ImageHandlesWrapped, String)],
    ) -> Element<'a, Message> {
        let font = self.config.font;
        let iter = content.into_iter().map(move |item| match item {
            ExplanationElement::List { numbered, items } => {
                let numbered_width = FONT_SIZE * ((items.len().ilog10() as f32 / 2.0).ceil() + 1.0);
                Column::from_iter(items.iter().enumerate().map(|(num, item)| {
                    let start = if *numbered {
                        text!("{}.", num + 1)
                            .font(font)
                            .align_x(Alignment::End)
                            .width(numbered_width)
                    } else {
                        text("•").font(font)
                    }
                    .color(self.extended_palette().secondary.base.color);
                    row![
                        start,
                        self.explanation_inner(item, kind, default_modifiers, images)
                    ]
                    .spacing(SPACING)
                    .into()
                }))
                .into()
            }
            ExplanationElement::Table {
                content,
                columns,
                rows,
            } => {
                if *rows == 0 || *columns == 0 {
                    return space().into();
                }

                let rows = (1..*rows).filter(|row| {
                    (0..*columns).any(|col| content.get(&(col, *row)).is_some_and(Option::is_some))
                });

                let columns = (0..*columns).map(|column| {
                    let header = content
                        .get(&(column, 0))
                        .and_then(Option::as_ref)
                        .map(|header| {
                            self.explanation_inner(
                                header,
                                kind,
                                Modifiers {
                                    heading: Some(Heading::H6),
                                    ..default_modifiers
                                },
                                images,
                            )
                        });
                    table::column(header, move |row| {
                        content
                            .get(&(column, row))
                            .and_then(Option::as_ref)
                            .map(|cell| {
                                self.explanation_inner(cell, kind, default_modifiers, images)
                            })
                    })
                    .width(if column == columns - 1 {
                        Length::FillPortion(3)
                    } else {
                        Length::FillPortion(1)
                    })
                });

                Table::new(columns, rows)
                    .padding(SPACING)
                    .separator(BORDER_WIDTH)
                    .into()
            }
            ExplanationElement::Unknown(html) => {
                text(html).color(self.palette().danger).font(font).into()
            }
            ExplanationElement::DescriptionList(descriptions) => {
                let descriptions = descriptions.iter().map(|Description { head, body }| {
                    let body = body.iter().map(|paragraph| {
                        self.explanation_inner(paragraph, kind, default_modifiers, images)
                    });
                    let body = Column::from_iter(body)
                        .spacing(SPACING)
                        .padding(padding::left(SPACING));
                    column![
                        self.explanation_inner(
                            head,
                            kind,
                            Modifiers {
                                heading: Some(Heading::H6),
                                ..default_modifiers
                            },
                            images
                        ),
                        body
                    ]
                    .into()
                });

                Column::from_iter(descriptions).spacing(SPACING).into()
            }
            ExplanationElement::Image { idx, description } => {
                let img = container(self.fetchable(
                    images[*idx].0.as_ref(),
                    "image",
                    Message::FetchImage(self.xkcd().num, ImageKind::Explanation(kind, *idx)),
                    MAX_EXPLANATION_WIDTH.into(),
                    (MAX_EXPLANATION_WIDTH / 2.0).into(),
                    |handles| {
                        viewer(handles.get(self.processing_enabled(self.xkcd().num)))
                            .min_scale(1.0)
                            .max_scale(MAX_IMG_SCALE)
                            .into()
                    },
                ))
                .padding(SPACING);
                if let Some(description) = description {
                    container(column![
                        container(img).center_x(Length::Fill).center_y(Length::Fill),
                        container(self.explanation_inner(
                            description,
                            kind,
                            default_modifiers,
                            images
                        ))
                        .center_x(Length::Fill)
                    ])
                    .padding(SPACING)
                    .style(|theme| container::Style {
                        text_color: Some(theme.palette().primary),
                        border: border::rounded(BORDER_RADIUS)
                            .width(BORDER_WIDTH)
                            .color(theme.extended_palette().secondary.base.color),
                        shadow: SHADOW,
                        background: Some(theme.palette().background.into()),
                        ..Default::default()
                    })
                    .into()
                } else {
                    img.into()
                }
            }
            ExplanationElement::Text(spans) => self.paragraph(spans, default_modifiers),
            ExplanationElement::BlockQuote(explanation_elements) => {
                let rule = rule::vertical(BORDER_WIDTH).style(|theme: &Theme| rule::Style {
                    color: theme.extended_palette().secondary.base.color,
                    radius: BORDER_RADIUS.into(),
                    fill_mode: FillMode::Full,
                    snap: true,
                });
                row![
                    rule,
                    self.explanation_inner(
                        explanation_elements,
                        kind,
                        Modifiers {
                            italic: true,
                            ..default_modifiers
                        },
                        images
                    )
                ]
                .spacing(5)
                .into()
            }
        });

        Column::from_iter(iter)
            .spacing(SPACING)
            .max_width(MAX_EXPLANATION_WIDTH)
            .into()
    }

    fn paragraph<'a>(
        &self,
        spans: impl IntoIterator<Item = &'a Span<'a>>,
        default_modifiers: Modifiers,
    ) -> Element<'a, Message> {
        let elements = spans.into_iter().map(move |span| {
            let Modifiers {
                bold,
                italic,
                big,
                underline,
                small,
                code,
                color,
                heading,
                strikethrough,
                ..
            } = default_modifiers | span.modifiers;

            let mut font = if code {
                Font::MONOSPACE
            } else {
                self.config.font
            };

            if bold || heading.is_some() {
                font.weight = Weight::Bold
            }

            if italic {
                font.style = font::Style::Italic
            }

            let size = match (heading, big, small) {
                (_, true, _) => FONT_SIZE * 2.0,
                (_, _, true) => SMOL_SIZE,
                (Some(heading), _, _) => heading.font_size(),
                _ => FONT_SIZE,
            };
            let weak = self.extended_palette().primary.weak.color;
            let color = match (color, &span.link) {
                (Some(color), _) => color,
                (_, None) => self.palette().text,
                (_, Some(Link::Xkcd(xkcd)))
                    if self.state.has_been_viewed(Viewable::Xkcd(*xkcd)) =>
                {
                    weak
                }
                (_, Some(Link::SelfLink)) => weak,
                (_, Some(Link::ExplainXkcdUrl(url) | Link::ExplainXkcdUrl(url)))
                    if self.state.has_been_viewed(Viewable::Url(url.clone())) =>
                {
                    weak
                }
                _ => self.palette().primary,
            };

            iced_selection::span(span.text.to_string())
                .font(font)
                .color(color)
                .underline(underline)
                .strikethrough(strikethrough)
                .size(size)
                .link_maybe(span.link.clone())
        });

        Rich::from_iter(elements)
            .on_link_click(Message::LinkClicked)
            .on_link_hover(Message::LinkHover)
            .on_hover_lost(Message::HoverEnd)
            .wrapping(text::Wrapping::WordOrGlyph)
            .into()
    }

    fn button<'a, F: Fn(&Theme, button::Status) -> button::Style + 'a>(
        icon: Icon,
        style: F,
    ) -> Button<'a, Message> {
        let content = text(icon.unicode())
            .font(Font::with_name("lucide"))
            .shaping(text::Shaping::Advanced);
        button(content)
            .style(move |theme, status| {
                let mut style = style(theme, status);
                style.shadow = SHADOW;
                style.border.radius = BORDER_RADIUS.into();
                style
            })
            .padding(padding::vertical(SPACING).horizontal(SPACING * 2.0))
    }

    fn fetchable<'a, T>(
        &self,
        fetchable: Option<&'a Result<T, String>>,
        name: &'a str,
        msg: Message,
        width: Length,
        height: Length,
        view: impl FnOnce(&'a T) -> Element<'a, Message>,
    ) -> Element<'a, Message> {
        match fetchable {
            Some(Ok(fetched)) => view(fetched),
            Some(Err(error)) => container(
                row![
                    Self::button(Icon::RefreshCw, button::primary).on_press(msg),
                    text!("Failed to fetch {name}: {error}")
                        .style(text::danger)
                        .font(self.config.font)
                ]
                .spacing(SPACING)
                .align_y(Alignment::Center),
            )
            .center_x(width)
            .center_y(height)
            .into(),
            _ => container(text!("Fetching {name}").font(self.config.font))
                .center_x(width)
                .center_y(height)
                .into(),
        }
    }

    fn processing_enabled(&self, comic: u32) -> bool {
        self.state
            .processing_enabled(self.config.process_image_by_default, comic)
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

pub struct OptionArrayChunks<I: Iterator, const N: usize>(Peekable<I>);

impl<I: Iterator, const N: usize> Iterator for OptionArrayChunks<I, N> {
    type Item = [Option<I::Item>; N];

    fn next(&mut self) -> Option<Self::Item> {
        self.0.peek()?;

        Some(array::from_fn(|_| self.0.next()))
    }
}

impl Icekcd {
    pub fn view(&self) -> Element<'_, Message> {
        match self {
            Icekcd::InitFailure(report, config, _) => container(
                row![
                    Running::button(Icon::RefreshCw, button::primary).on_press(Message::Reboot),
                    text!("Failed to load app: {report}")
                        .font(config.as_ref().unwrap_or(&Config::default()).font)
                        .style(text::danger)
                ]
                .align_y(Alignment::Center)
                .spacing(SPACING),
            )
            .center(Length::Fill)
            .into(),
            Icekcd::Running(running) => running.view(),
            Icekcd::Starting(config, _) => container(
                text!("Starting").font(config.as_ref().unwrap_or(&Config::default()).font),
            )
            .center(Length::Fill)
            .into(),
        }
    }
}
