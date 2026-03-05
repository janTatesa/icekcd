use iced::{
    Event,
    keyboard::{self, Key, Modifiers, key::Named},
    mouse, window,
};

use crate::Message;

pub fn handle_iced_event(event: Event) -> Option<Message> {
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

fn handle_key(key: Key, modifiers: Modifiers) -> Option<Message> {
    use Key as K;
    use Message as MSG;
    use Modifiers as M;
    use Named as N;
    Some(match (key.as_ref(), modifiers) {
        (K::Character("r"), M::NONE) => MSG::GoToRandom,
        (K::Character("b"), M::NONE) => MSG::ToggleBookmark,
        (K::Character("g"), M::NONE) => MSG::GoToBookmark,
        (K::Character("p"), M::NONE) => MSG::ToggleProcessImage,
        (K::Character("v"), M::CTRL) => MSG::Paste,
        (K::Character("c"), M::CTRL) => MSG::Copy,
        (K::Character("f"), M::NONE) => MSG::ToggleFavorite,
        (K::Character("f"), M::CTRL) => MSG::ToggleShowFavorites,
        (K::Character("e"), M::NONE) => MSG::ToggleShowExplanation,
        (K::Character("e"), M::CTRL) => MSG::OpenExplanationInBrowser,
        (K::Character("+"), M::CTRL) => MSG::ScaleUp,
        (K::Character("-"), M::CTRL) => MSG::ScaleDown,
        (K::Character("0"), M::CTRL) => MSG::ScaleReset,
        (K::Character("o"), M::CTRL) => MSG::OpenInBrowser,
        (K::Named(N::ArrowDown), M::NONE) => MSG::ScrollDown,
        (K::Named(N::ArrowUp), M::NONE) => MSG::ScrollUp,
        (K::Named(N::ArrowLeft), M::CTRL) => MSG::DragSplitLeft,
        (K::Named(N::ArrowRight), M::CTRL) => MSG::DragSplitRight,
        (K::Named(N::End), M::NONE) => MSG::GoToLatest,
        (K::Named(N::ArrowLeft), M::NONE) => MSG::GoToPrevious,
        (K::Named(N::ArrowRight), M::NONE) => MSG::GoToNext,
        (K::Named(N::ArrowRight), M::ALT) => MSG::HistoryNext,
        (K::Named(N::ArrowLeft), M::ALT) => MSG::HistoryBack,
        (K::Named(N::Escape), M::NONE) => MSG::ClosePopup,
        (K::Named(N::PageUp), M::NONE) => MSG::ScrollToStart,
        (K::Named(N::PageDown), M::NONE) => MSG::ScrollToEnd,
        (K::Named(N::Home), M::NONE) => MSG::GoToComic(1),
        _ => return None,
    })
}
