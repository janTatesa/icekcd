use std::{
    alloc::{self, Layout},
    borrow::Cow,
    cmp::max,
    collections::HashMap,
    mem::{self, ManuallyDrop},
    ops::BitOr,
    ptr::NonNull,
};

use ego_tree::NodeRef;
use iced::Color;
use scraper::{CaseSensitivity, ElementRef, Html, Node, Selector};

use crate::{ExplanationKind, FONT_SIZE, ImageHandlesWrapped};

// TODO: use yoke
#[derive(Debug)]
pub struct Explanation {
    ptr: NonNull<Html>,
    elements: ManuallyDrop<Vec<ExplanationElement<'static>>>,
    pub contains_unknown: bool,
    pub images: Vec<(ImageHandlesWrapped, String)>,
}

unsafe impl Send for Explanation {}
unsafe impl Sync for Explanation {}

impl Explanation {
    pub fn new(src: &str, kind: ExplanationKind) -> Option<Self> {
        let html = Box::leak(Box::new(Html::parse_document(src)));
        let ptr = NonNull::from_mut(html);

        let mut contains_unknown = false;
        let mut images = vec![];
        let modifiers = Modifiers::default();
        let elements = match kind {
            ExplanationKind::Comic => {
                let elements = html
                    .select(&Selector::parse("#Explanation").unwrap())
                    .next()
                    .or_else(|| {
                        html.select(&Selector::parse("#Eggsplanation").unwrap())
                            .next()
                    })?
                    .parent()?
                    .next_siblings()
                    .take_while(|node| {
                        node.value()
                            .as_element()
                            .is_none_or(|elem| elem.name() != "h2" && elem.name() != "h1")
                    });

                scrape_elements(elements, &mut contains_unknown, &mut images, modifiers)
            }
            ExplanationKind::Article => {
                let elements = html
                    .select(&Selector::parse(".mw-parser-output").unwrap())
                    .next()?
                    .children();
                scrape_elements(elements, &mut contains_unknown, &mut images, modifiers)
            }
        };

        Some(Self {
            elements: ManuallyDrop::new(elements),
            contains_unknown,
            images,
            ptr,
        })
    }

    pub fn elements<'a>(&'a self) -> &'a [ExplanationElement<'a>] {
        &self.elements
    }
}

impl Drop for Explanation {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.elements);
            let ptr = self.ptr.as_ptr() as *mut u8;
            let layout = Layout::new::<Html>();
            alloc::dealloc(ptr, layout)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExplanationElement<'a> {
    Text(Vec<Span<'a>>),
    BlockQuote(Vec<ExplanationElement<'a>>),
    List {
        numbered: bool,
        items: Vec<Vec<ExplanationElement<'a>>>,
    },
    Table {
        content: HashMap<(usize, usize), Option<Vec<ExplanationElement<'a>>>>,
        columns: usize,
        rows: usize,
    },
    DescriptionList(Vec<Description<'a>>),
    Unknown(Cow<'a, str>),
    Image {
        idx: usize,
        description: Option<Vec<ExplanationElement<'a>>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span<'a> {
    pub text: Cow<'a, str>,
    pub modifiers: Modifiers,
    pub link: Option<Link>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heading {
    H2 = 2,
    H3,
    H4,
    H5,
    H6,
}

impl Heading {
    pub fn font_size(self) -> f32 {
        FONT_SIZE + ((7 - self as u8) * 2) as f32
    }

    fn scraped(self, modifiers: Modifiers) -> ScrapeElementOut<'static> {
        Modifiers {
            heading: Some(self),
            ..modifiers
        }
        .into()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Description<'a> {
    pub head: Vec<ExplanationElement<'a>>,
    pub body: Vec<Vec<ExplanationElement<'a>>>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct Modifiers {
    pub bold: bool,
    pub italic: bool,
    pub big: bool,
    pub underline: bool,
    pub code: bool,
    pub list: bool,
    pub small: bool,
    pub strikethrough: bool,
    pub heading: Option<Heading>,
    pub color: Option<Color>,
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            bold: self.bold | rhs.bold,
            italic: self.italic | rhs.italic,
            big: self.big | rhs.big,
            underline: self.underline | rhs.underline,
            code: self.code | rhs.code,
            heading: self.heading.or(rhs.heading),
            color: self.color.or(rhs.color),
            list: self.list | rhs.list,
            small: self.small | rhs.small,
            strikethrough: self.strikethrough | rhs.strikethrough,
        }
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    SelfLink,
    Xkcd(u32),
    ExplainXkcdUrl(String),
    Url(String),
}

fn scrape_elements<'a>(
    contents: impl Iterator<Item = NodeRef<'a, Node>> + 'a,
    contains_unknown: &mut bool,
    images: &mut Vec<(ImageHandlesWrapped, String)>,
    modifiers: Modifiers,
) -> Vec<ExplanationElement<'a>> {
    fn push_spans<'a>(elements: &mut Vec<ExplanationElement<'a>>, mut spans: Vec<Span<'a>>) {
        while spans.last().is_some_and(|span| span.text.trim() == "") {
            spans.pop();
        }

        while spans.first().is_some_and(|span| span.text.trim() == "") {
            spans.remove(0);
        }

        if !spans.is_empty() {
            elements.push(ExplanationElement::Text(spans));
        }
    }

    let mut node_iter = NodeIter {
        children: Box::new(contents),
        inner: None,
        modifiers,
        peeked: None,
    };
    let mut spans: Vec<Span<'a>> = vec![];
    let mut elements = vec![];
    while let Some((node, modifiers)) = node_iter.next() {
        match scrape_element(node, contains_unknown, images, modifiers) {
            Some(ScrapeElementOut::Span(span)) => spans.push(span),
            Some(ScrapeElementOut::Element(element)) => {
                if !spans.is_empty() {
                    push_spans(&mut elements, mem::take(&mut spans));
                }

                elements.push(element);
            }
            Some(ScrapeElementOut::Continue) => continue,
            Some(ScrapeElementOut::Modifiers(modifiers)) => node_iter.set_inner(NodeIter {
                children: Box::new(node.children()),
                inner: None,
                modifiers,
                peeked: None,
            }),
            Some(ScrapeElementOut::StandaloneListItem(item)) => {
                let mut items = vec![item];
                loop {
                    match node_iter.peek() {
                        Some(node)
                            if node
                                .0
                                .value()
                                .as_text()
                                .is_some_and(|text| text.trim() == "") => {}
                        Some(node)
                            if node
                                .0
                                .value()
                                .as_element()
                                .is_some_and(|elem| elem.name() == "li") =>
                        {
                            items.push(scrape_elements(
                                node.0.children(),
                                contains_unknown,
                                images,
                                node.1,
                            ));
                        }
                        _ => break,
                    }

                    node_iter.next();
                }

                elements.push(ExplanationElement::List {
                    numbered: false,
                    items,
                })
            }
            None => {
                elements.push(ExplanationElement::Unknown(match ElementRef::wrap(node) {
                    Some(elem) => elem.html().into(),
                    _ if node.value().is_comment() => continue,
                    None => format!("{:?}", node.value()).into(),
                }));
                *contains_unknown = true;
            }
        }
    }

    push_spans(&mut elements, spans);

    elements
}

struct NodeIter<'a> {
    children: Box<dyn Iterator<Item = NodeRef<'a, Node>> + 'a>,
    inner: Option<Box<Self>>,
    modifiers: Modifiers,
    peeked: Option<(NodeRef<'a, Node>, Modifiers)>,
}

impl<'a> NodeIter<'a> {
    fn set_inner(&mut self, new_inner: Self) {
        match &mut self.inner {
            Some(inner) => inner.set_inner(new_inner),
            None => self.inner = Some(Box::new(new_inner)),
        }
    }

    fn peek(&mut self) -> Option<(NodeRef<'a, Node>, Modifiers)> {
        self.peeked = self.next();
        self.peeked
    }
}

impl<'a> Iterator for NodeIter<'a> {
    type Item = (NodeRef<'a, Node>, Modifiers);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(peeked) = self.peeked.take() {
            return Some(peeked);
        }

        if let Some(iter) = &mut self.inner
            && let Some(node) = iter.next()
        {
            return Some(node);
        } else {
            self.inner = None;
        }

        Some((self.children.next()?, self.modifiers))
    }
}

enum ScrapeElementOut<'a> {
    Span(Span<'a>),
    Element(ExplanationElement<'a>),
    StandaloneListItem(Vec<ExplanationElement<'a>>),
    Modifiers(Modifiers),
    Continue,
}

impl<'a> From<ExplanationElement<'a>> for ScrapeElementOut<'a> {
    fn from(value: ExplanationElement<'a>) -> Self {
        Self::Element(value)
    }
}

impl<'a> From<Span<'a>> for ScrapeElementOut<'a> {
    fn from(value: Span<'a>) -> Self {
        Self::Span(value)
    }
}

impl From<Modifiers> for ScrapeElementOut<'_> {
    fn from(value: Modifiers) -> Self {
        Self::Modifiers(value)
    }
}

fn scrape_element<'a>(
    node: NodeRef<'a, Node>,
    contains_unknown: &mut bool,
    images: &mut Vec<(ImageHandlesWrapped, String)>,
    modifiers: Modifiers,
) -> Option<ScrapeElementOut<'a>> {
    if let Node::Text(text) = node.value() {
        return Some(
            Span {
                text: (&**text).into(),
                modifiers,
                link: None,
            }
            .into(),
        );
    }

    let mut contents = node.children();
    let element = node.value().as_element()?;
    Some(match element.name() {
        "blockquote" => ExplanationElement::BlockQuote(scrape_elements(
            contents,
            contains_unknown,
            images,
            modifiers,
        ))
        .into(),
        "dl" => {
            let mut descriptions = vec![];
            for child in contents {
                if child.value().as_text().is_some_and(|text| &**text == "\n") {
                    continue;
                }

                match child.value().as_element()?.name() {
                    "dt" => {
                        descriptions.push(Description {
                            head: scrape_elements(
                                child.children(),
                                contains_unknown,
                                images,
                                modifiers,
                            ),
                            body: vec![],
                        });
                    }
                    "dd" => {
                        let desc = match descriptions.last_mut() {
                            Some(last) => last,
                            None => {
                                descriptions.push(Description::default());
                                descriptions.last_mut()?
                            }
                        };

                        desc.body.push(scrape_elements(
                            child.children(),
                            contains_unknown,
                            images,
                            modifiers,
                        ))
                    }
                    _ => return None,
                }
            }

            ExplanationElement::DescriptionList(descriptions).into()
        }
        "ul" => ExplanationElement::List {
            numbered: false,
            items: contents
                .map(|node| {
                    scrape_elements(
                        node.children(),
                        contains_unknown,
                        images,
                        Modifiers {
                            list: true,
                            ..modifiers
                        },
                    )
                })
                .filter(|item| !item.is_empty())
                .collect(),
        }
        .into(),
        "ol" => ExplanationElement::List {
            numbered: true,
            items: contents
                .map(|node| {
                    scrape_elements(
                        node.children(),
                        contains_unknown,
                        images,
                        Modifiers {
                            list: true,
                            ..modifiers
                        },
                    )
                })
                .filter(|item| !item.is_empty())
                .collect(),
        }
        .into(),
        "li" if !modifiers.list => ScrapeElementOut::StandaloneListItem(scrape_elements(
            contents,
            contains_unknown,
            images,
            modifiers,
        )),
        "div"
            if node.children().any(|node| {
                node.value()
                    .as_element()
                    .is_some_and(|elem| elem.has_class("notice", CaseSensitivity::CaseSensitive))
            }) =>
        {
            ScrapeElementOut::Continue
        }
        "table" => {
            let mut content = HashMap::new();
            let mut columns = 0;
            let mut rows = 0;
            for (row_num, row) in contents
                .find(|child| child.value().is_element())?
                .children()
                .filter(|child| child.value().is_element())
                .enumerate()
            {
                rows += 1;
                let mut col_num = 0;
                for cell in row.children().filter_map(ElementRef::wrap) {
                    while content.contains_key(&(col_num, row_num)) {
                        col_num += 1;
                    }

                    let cell_content =
                        scrape_elements(cell.children(), contains_unknown, images, modifiers);
                    let colspan = match cell.attr("colspan") {
                        Some(span) => span.strip_suffix(";").unwrap_or(span).parse().ok()?,
                        None => 1,
                    };

                    let rowspan = match cell.attr("rowspan") {
                        Some(span) => span.strip_suffix(";").unwrap_or(span).parse().ok()?,
                        None => 1,
                    };

                    for col_offset in 0..colspan {
                        for row_offset in 0..rowspan {
                            let idx = (col_num + col_offset, row_num + row_offset);
                            content.insert(idx, None);
                        }
                    }

                    if !cell_content.is_empty() {
                        content.insert((col_num, row_num), Some(cell_content));
                    }

                    col_num += colspan;
                }

                columns = max(columns, col_num);
            }

            ExplanationElement::Table {
                content,
                columns,
                rows,
            }
            .into()
        }
        "h2" => Heading::H2.scraped(modifiers),
        "h3" => Heading::H3.scraped(modifiers),
        "h4" => Heading::H4.scraped(modifiers),
        "h5" => Heading::H5.scraped(modifiers),
        "h6" => Heading::H6.scraped(modifiers),
        "div"
            if node
                .value()
                .as_element()?
                .has_class("thumb", CaseSensitivity::CaseSensitive) =>
        {
            let mut children = contents.next()?.children();
            let anchor = children.find_map(ElementRef::wrap)?;
            let img = anchor.attr("href").or_else(|| {
                let attr = anchor
                    .children()
                    .next()?
                    .value()
                    .as_element()?
                    .attr("src")?;
                Some(attr)
            })?;
            let img_url = format!("https://explainxkcd.com{img}");
            images.push((None, img_url));
            let description = children
                .find(|child| child.value().is_element())
                .map(|content| {
                    let content = content.children().filter(|node| {
                        node.value().as_element().is_none_or(|elem| {
                            !elem.has_class("magnify", CaseSensitivity::CaseSensitive)
                        })
                    });

                    scrape_elements(content, contains_unknown, images, modifiers)
                });

            ExplanationElement::Image {
                idx: images.len() - 1,
                description,
            }
            .into()
        }
        "div"
            if element.has_class("Bug6200", CaseSensitivity::CaseSensitive)
                || element.has_class("templatequotecite", CaseSensitivity::CaseSensitive) =>
        {
            Modifiers {
                italic: true,
                ..modifiers
            }
            .into()
        }
        "i" | "q" | "em" | "var" => Modifiers {
            italic: true,
            ..modifiers
        }
        .into(),
        "b" => Modifiers {
            bold: true,
            ..modifiers
        }
        .into(),
        "u" | "ins" => Modifiers {
            underline: true,
            ..modifiers
        }
        .into(),
        "s" | "strike" | "del" => Modifiers {
            strikethrough: true,
            ..modifiers
        }
        .into(),
        "big" => Modifiers {
            big: true,
            ..modifiers
        }
        .into(),
        "sup" | "sub" | "small" => Modifiers {
            small: true,
            ..modifiers
        }
        .into(),
        "pre" | "code" => Modifiers {
            code: true,
            ..modifiers
        }
        .into(),
        "span" if element.has_class("mw-editsection", CaseSensitivity::CaseSensitive) => {
            ScrapeElementOut::Continue
        }
        "span" | "font" | "p" | "cite" | "li" | "div" | "center" => {
            fn parse_rgb_color(str: &str) -> Option<Color> {
                let mut iter = str
                    .trim()
                    .strip_prefix("rgb(")?
                    .strip_suffix(")")?
                    .split(",")
                    .flat_map(|s| s.trim().parse().ok());
                let (r, g, b) = (iter.next()?, iter.next()?, iter.next()?);
                Some(Color::from_rgb8(r, g, b))
            }

            let color = element
                .attr("color")
                .or_else(|| element.attr("style")?.strip_prefix("color:"))
                .and_then(|str| str.parse().ok().or_else(|| parse_rgb_color(str)));

            Modifiers {
                color: color.or(modifiers.color),
                ..modifiers
            }
            .into()
        }
        "tt" | "hr" | "br" | "wbr" => ScrapeElementOut::Continue,
        "a" if element.has_class("image", CaseSensitivity::CaseSensitive) => {
            let img = node
                .children()
                .find_map(|node| node.value().as_element())?
                .attr("src")?;
            let img_url = format!("https://explainxkcd.com{img}");
            images.push((None, img_url));
            ExplanationElement::Image {
                idx: images.len() - 1,
                description: None,
            }
            .into()
        }
        "img" => {
            let img = element.attr("src")?;
            let img_url = format!("https://explainxkcd.com{img}");
            images.push((None, img_url));
            ExplanationElement::Image {
                idx: images.len() - 1,
                description: None,
            }
            .into()
        }
        "abbr" => Span {
            text: element.attr("title")?.into(),
            modifiers,
            link: None,
        }
        .into(),
        "a" => {
            let mut text = String::new();

            for elem in scrape_elements(contents, contains_unknown, images, modifiers) {
                match elem {
                    ExplanationElement::Text(spans) => {
                        text.push_str(&spans.iter().map(|span| &*span.text).collect::<String>())
                    }
                    ExplanationElement::Unknown(_) => return None,
                    _ => {}
                };
            }

            let text = text.into();
            if element.has_class("selflink", CaseSensitivity::CaseSensitive) {
                let span = Span {
                    text,
                    modifiers,
                    link: Some(Link::SelfLink),
                };
                return Some(span.into());
            }

            let url = element.attr("href")?;

            if let Some(base) = url.strip_prefix("/wiki/index.php/") {
                if let Some(num) = base.split(":").next().and_then(|num| num.parse().ok()) {
                    let span = Span {
                        text,
                        modifiers,
                        link: Some(Link::Xkcd(num)),
                    };
                    return Some(span.into());
                }

                let url = format!("https://explainxkcd.com{url}");
                Span {
                    text,
                    modifiers,
                    link: Some(Link::ExplainXkcdUrl(url)),
                }
                .into()
            } else {
                Span {
                    text,
                    modifiers,
                    link: Some(Link::Url(url.to_string())),
                }
                .into()
            }
        }

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use isahc::ReadResponseExt;

    use crate::{ExplanationKind, explanation::Explanation, xkcd::Xkcd};

    #[test]
    fn test_scraping() {
        let max = smol::block_on(Xkcd::get_latest()).unwrap().num;
        let start: u32 = option_env!("TEST_START").unwrap_or("1").parse().unwrap();
        for num in start..=max {
            println!("Fetching {num}");
            let mut response = isahc::get(format!("https://explainxkcd.com/{num}")).unwrap();
            assert!(!(response.status().is_client_error() || response.status().is_server_error()));
            let src = response.text().unwrap();
            println!("Scraping {num}");
            let explanation = Explanation::new(&src, ExplanationKind::Comic).unwrap();
            if explanation.contains_unknown {
                dbg!(&explanation);
                panic!();
            }
        }
    }
}
