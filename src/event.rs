use iced::{
    Event,
    keyboard::{self, Key, Modifiers, key::Named},
    mouse, window,
};

use crate::Message;

pub(super) fn handle_iced_event(event: Event) -> Option<Message> {
    Some(match event {
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            return handle_key(key, modifiers);
        }
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Forward)) => Message::HistoryNext,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Back)) => Message::HistoryBack,
        Event::Window(window::Event::Focused) => Message::ReloadState,
        _ => return None,
    })
}

pub fn handle_key(key: Key, modifiers: Modifiers) -> Option<Message> {
    use Key as K;
    use Modifiers as M;
    Some(match (key.as_ref(), modifiers) {
        (K::Character("r"), M::NONE) => Message::GoToRandom,
        (K::Character("b"), M::NONE) => Message::ToggleBookmark,
        (K::Character("g"), M::NONE) => Message::GoToBookmark,
        (K::Character("p"), M::NONE) => Message::ToggleProcessImage,
        (K::Character("v"), M::CTRL) => Message::Paste,
        (K::Character("c"), M::CTRL) => Message::Copy,
        (K::Character("f"), M::NONE) => Message::ToggleFavorite,
        (K::Character("f"), M::CTRL) => Message::ToggleShowFavorites,
        (K::Character("e"), M::NONE) => Message::ToggleShowExplanation,
        (K::Character("e"), M::CTRL) => Message::OpenExplanationInBrowser,
        (K::Character("+"), M::CTRL) => Message::ScaleUp,
        (K::Character("-"), M::CTRL) => Message::ScaleDown,
        (K::Character("0"), M::CTRL) => Message::ScaleReset,
        (K::Character("o"), M::CTRL) => Message::OpenInBrowser,
        (K::Named(Named::ArrowDown), M::NONE) => Message::ScrollDown,
        (K::Named(Named::ArrowUp), M::NONE) => Message::ScrollUp,
        (K::Named(Named::ArrowLeft), M::CTRL) => Message::DragSplitLeft,
        (K::Named(Named::ArrowRight), M::CTRL) => Message::DragSplitRight,
        (K::Named(Named::End), M::NONE) => Message::GoToLatest,
        (K::Named(Named::ArrowLeft), M::NONE) => Message::GoToPrevious,
        (K::Named(Named::ArrowRight), M::NONE) => Message::GoToNext,
        (K::Named(Named::ArrowRight), M::ALT) => Message::HistoryNext,
        (K::Named(Named::ArrowLeft), M::ALT) => Message::HistoryBack,
        (K::Named(Named::Escape), M::NONE) => Message::ClosePopup,
        (K::Named(Named::PageUp), M::NONE) => Message::ScrollToStart,
        (K::Named(Named::PageDown), M::NONE) => Message::ScrollToEnd,
        (K::Named(Named::Home), M::NONE) => Message::GoToComic(1),
        _ => return None,
    })
}
