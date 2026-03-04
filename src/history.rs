use serde::{Deserialize, Serialize};

use crate::xkcd::Xkcd;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct History {
    history: Vec<HistoryEntry>,
    index: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HistoryEntry {
    pub xkcd: Xkcd,
    pub article: Option<String>,
}

impl History {
    pub fn new(xkcd: Xkcd) -> Self {
        let history = vec![HistoryEntry {
            xkcd,
            article: None,
        }];
        Self { history, index: 0 }
    }

    pub fn current_entry(&self) -> &HistoryEntry {
        &self.history[self.index]
    }

    pub fn backward(&mut self) -> bool {
        if self.can_go_backward() {
            self.index -= 1;
            return true;
        }

        false
    }

    pub fn forward(&mut self) -> bool {
        if self.can_go_forward() {
            self.index += 1;
            return true;
        }

        false
    }

    pub fn can_go_backward(&self) -> bool {
        self.index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.index < self.history.len() - 1
    }

    pub fn open(&mut self, entry: HistoryEntry, max_size: usize) {
        if self.history.len() > self.index + 1 {
            self.history.drain((self.index + 1)..);
        }

        if self.history.len() == max_size {
            self.history.remove(0);
        } else {
            self.index += 1;
        }

        self.history.push(entry);
    }
}
