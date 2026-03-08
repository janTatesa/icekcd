use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use std::fs;
use yanet::{OptionExt, Result};

use crate::{
    history::{History, HistoryEntry},
    xkcd::Xkcd,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct State {
    latest_xkcd_viewed: u32,
    bookmarked: Option<u32>,
    enable_processing: HashMap<u32, bool>,
    favorites: Vec<Xkcd>,
    show_explanation: bool,
    show_favorites: bool,
    split: f32,
    scale: f32,
    viewed: HashSet<Viewable>,
    history: History,
    //TODO: favorites: HashSet<u32>,
}

#[derive(PartialEq, Eq, Hash, Deserialize, Serialize, Debug, Clone)]
pub enum Viewable {
    Xkcd(u32),
    Url(String),
}

impl State {
    fn path() -> Result<PathBuf> {
        let mut path = dirs::data_dir().ok_or_yanet("Cannot find config dir")?;
        path.push("icekcd");
        if !path.exists() {
            fs::create_dir(&path)?;
        }

        path.push("state.json");
        Ok(path)
    }

    pub fn load(xkcd: Xkcd, open_xkcd: bool, max_history_size: usize) -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self {
                latest_xkcd_viewed: xkcd.num,
                bookmarked: None,
                enable_processing: HashMap::new(),
                show_explanation: false,
                split: 0.5,
                scale: 1.0,
                history: History::new(xkcd),
                viewed: HashSet::new(),
                favorites: Vec::new(),
                show_favorites: false,
            });
        }

        let mut state: Self = serde_json::from_str(&fs::read_to_string(path)?)?;
        if open_xkcd && xkcd.num != state.history().current_entry().xkcd.num {
            state.open_xkcd(xkcd, max_history_size)?
        }

        Ok(state)
    }

    pub fn reload(&mut self) -> Result<()> {
        let path = Self::path()?;
        if path.exists() {
            *self = serde_json::from_str(&fs::read_to_string(path)?)?;
        }

        Ok(())
    }

    fn save(&self) -> Result<()> {
        fs::write(Self::path()?, serde_json::to_string(&self)?)?;
        Ok(())
    }

    pub fn toggle_bookmark(&mut self, comic: u32) -> Result<()> {
        self.bookmarked = if self.bookmarked == Some(comic) {
            None
        } else {
            Some(comic)
        };

        self.save()
    }

    pub fn bookmarked(&self) -> Option<u32> {
        self.bookmarked
    }

    pub fn toggle_processing(&mut self, default: bool) -> Result<()> {
        let enable = self
            .enable_processing
            .get(&self.history.current_entry().xkcd.num)
            .unwrap_or(&default);
        self.enable_processing
            .insert(self.history.current_entry().xkcd.num, !enable);
        self.save()
    }

    pub fn processing_enabled(&self, default: bool, comic: u32) -> bool {
        *self.enable_processing.get(&comic).unwrap_or(&default)
    }

    pub fn split(&self) -> f32 {
        self.split
    }

    pub fn drag_split(&mut self, split: f32) -> Result<()> {
        self.split = split;
        self.save()
    }

    pub fn show_explanation(&self) -> bool {
        self.show_explanation
    }

    pub fn toggle_show_explanation(&mut self) -> Result<()> {
        self.show_explanation = !self.show_explanation;
        self.save()
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn set_scale(&mut self, scale: f32) -> Result<()> {
        self.scale = scale;
        self.save()
    }

    pub fn latest_xkcd_viewed(&self) -> u32 {
        self.latest_xkcd_viewed
    }

    pub fn open_xkcd(&mut self, xkcd: Xkcd, max_history_size: usize) -> Result<()> {
        if self.latest_xkcd_viewed < xkcd.num {
            self.latest_xkcd_viewed = xkcd.num;
        }

        self.viewed.insert(Viewable::Xkcd(xkcd.num));
        let entry = HistoryEntry {
            xkcd,
            article: None,
        };
        self.history.open(entry, max_history_size);
        self.save()
    }

    pub fn history_forward(&mut self) -> (bool, Result<()>) {
        (self.history.forward(), self.save())
    }

    pub fn history_backward(&mut self) -> (bool, Result<()>) {
        (self.history.backward(), self.save())
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn has_been_viewed(&self, viewable: Viewable) -> bool {
        self.viewed.contains(&viewable)
    }

    pub fn open_url(&mut self, url: String) -> Result<()> {
        self.viewed.insert(Viewable::Url(url));
        self.save()
    }

    pub fn open_article(&mut self, url: String, max_history_size: usize) -> Result<()> {
        let entry = HistoryEntry {
            xkcd: self.history().current_entry().xkcd.clone(),
            article: Some(url.clone()),
        };

        self.viewed.insert(Viewable::Url(url));
        self.history.open(entry, max_history_size);
        self.save()
    }

    pub fn close_article(&mut self, max_history_size: usize) -> Result<()> {
        let entry = HistoryEntry {
            xkcd: self.history().current_entry().xkcd.clone(),
            article: None,
        };

        self.history.open(entry, max_history_size);
        self.save()
    }

    pub fn toggle_favorite(&mut self) -> Result<()> {
        let current = &self.history.current_entry().xkcd;
        if let Some(pos) = self.favorites.iter().position(|xkcd| current == xkcd) {
            self.favorites.remove(pos);
        } else {
            self.favorites.push(current.clone());
        }

        self.save()
    }

    pub fn favorites(&self) -> &[Xkcd] {
        &self.favorites
    }

    pub fn toggle_show_favorites(&mut self) -> Result<()> {
        self.show_favorites = !self.show_favorites;
        self.save()
    }

    pub fn show_favorites(&self) -> bool {
        self.show_favorites
    }
}
