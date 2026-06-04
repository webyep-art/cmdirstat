// Copyright (c) 2024 webyep
// Licensed under the MIT License.

use crate::core::tree::Node;
use ratatui::widgets::ListState;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU64};

pub enum AppState {
    Selecting(ListState, Vec<PathBuf>),
    Browsing(BrowsingState),
}

#[derive(Clone)]
pub struct DeletingItem {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortColumn {
    Size,
    Name,
    Modified,
    Accessed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

pub struct BrowsingState {
    pub root: Arc<RwLock<Node>>,
    pub is_scanning: Arc<AtomicBool>,
    pub items_scanned: Arc<AtomicU64>,
    pub skipped_files: Arc<AtomicU64>,
    pub start_time: std::time::Instant,
    pub path_indices: Vec<usize>,
    pub current_path: Vec<String>,
    pub list_state: ListState,
    pub cached_children: Vec<String>,
    pub deleting_item: Option<DeletingItem>,
    pub deletion_error: Option<String>,
    pub show_help: bool,
    pub search_active: bool,
    pub search_query: String,
    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
    pub is_ssd: Option<bool>,
}

pub struct App {
    pub state: AppState,
}

impl App {
    pub fn new_selecting(drives: Vec<PathBuf>) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            state: AppState::Selecting(list_state, drives),
        }
    }

    pub fn start_browsing(&mut self, path: PathBuf) {
        let root_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("/")
            .to_string();
        
        let is_ssd = crate::core::selector::detect_is_ssd(&path);
        let root = Arc::new(RwLock::new(Node::new(root_name, path, true)));
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let browsing_state = BrowsingState {
            root,
            is_scanning: Arc::new(AtomicBool::new(true)),
            items_scanned: Arc::new(AtomicU64::new(0)),
            skipped_files: Arc::new(AtomicU64::new(0)),
            start_time: std::time::Instant::now(),
            path_indices: vec![0],
            current_path: Vec::new(),
            list_state,
            cached_children: Vec::new(),
            deleting_item: None,
            deletion_error: None,
            show_help: false,
            search_active: false,
            search_query: String::new(),
            sort_column: SortColumn::Size,
            sort_direction: SortDirection::Descending,
            is_ssd,
        };
        
        self.state = AppState::Browsing(browsing_state);
        if let AppState::Browsing(s) = &mut self.state {
            s.update_cache();
        }
    }

    pub fn next(&mut self) {
        match &mut self.state {
            AppState::Selecting(state, drives) => {
                let i = match state.selected() {
                    Some(i) => if i >= drives.len() - 1 { 0 } else { i + 1 },
                    None => 0,
                };
                state.select(Some(i));
            }
            AppState::Browsing(s) => s.next(),
        }
    }

    pub fn previous(&mut self) {
        match &mut self.state {
            AppState::Selecting(state, drives) => {
                let i = match state.selected() {
                    Some(i) => if i == 0 { drives.len() - 1 } else { i - 1 },
                    None => 0,
                };
                state.select(Some(i));
            }
            AppState::Browsing(s) => s.previous(),
        }
    }

    pub fn enter(&mut self) -> Option<PathBuf> {
        match &mut self.state {
            AppState::Selecting(state, drives) => {
                if let Some(i) = state.selected() {
                    return Some(drives[i].clone());
                }
                None
            }
            AppState::Browsing(s) => {
                s.enter();
                None
            }
        }
    }

    pub fn back(&mut self) {
        let go_back_to_selecting = match &mut self.state {
            AppState::Selecting(_, _) => false,
            AppState::Browsing(s) => {
                if s.current_path.is_empty() {
                    s.is_scanning.store(false, std::sync::atomic::Ordering::SeqCst);
                    true
                } else {
                    s.back();
                    false
                }
            }
        };

        if go_back_to_selecting {
            let mut list_state = ratatui::widgets::ListState::default();
            list_state.select(Some(0));
            self.state = AppState::Selecting(list_state, crate::core::selector::get_available_drives());
        }
    }
}

impl BrowsingState {
    pub fn get_filtered_and_sorted_children<'a>(&self, node: &'a Node) -> Vec<&'a Node> {
        let children = Self::get_sorted_children(node, self.sort_column, self.sort_direction);
        if self.search_query.is_empty() {
            children
        } else {
            let query = self.search_query.to_lowercase();
            children.into_iter()
                .filter(|n| n.name.to_lowercase().contains(&query))
                .collect()
        }
    }

    pub fn get_sorted_children<'a>(node: &'a Node, column: SortColumn, direction: SortDirection) -> Vec<&'a Node> {
        let mut children: Vec<&'a Node> = node.children.values().collect();
        match column {
            SortColumn::Size => {
                children.sort_by(|a, b| {
                    let ord = a.size.cmp(&b.size);
                    if direction == SortDirection::Ascending { ord } else { ord.reverse() }
                });
            }
            SortColumn::Name => {
                children.sort_by(|a, b| {
                    let ord = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                    if direction == SortDirection::Ascending { ord } else { ord.reverse() }
                });
            }
            SortColumn::Modified => {
                children.sort_by(|a, b| {
                    let ord = a.modified.cmp(&b.modified);
                    if direction == SortDirection::Ascending { ord } else { ord.reverse() }
                });
            }
            SortColumn::Accessed => {
                children.sort_by(|a, b| {
                    let ord = a.accessed.cmp(&b.accessed);
                    if direction == SortDirection::Ascending { ord } else { ord.reverse() }
                });
            }
        }
        children
    }

    pub fn cycle_sort_column(&mut self) {
        self.sort_column = match self.sort_column {
            SortColumn::Size => SortColumn::Name,
            SortColumn::Name => SortColumn::Modified,
            SortColumn::Modified => SortColumn::Accessed,
            SortColumn::Accessed => SortColumn::Size,
        };
        self.update_cache();
    }

    pub fn toggle_sort_direction(&mut self) {
        self.sort_direction = match self.sort_direction {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        };
        self.update_cache();
    }

    pub fn update_cache(&mut self) {
        let root = self.root.read().unwrap();
        let mut curr = &*root;
        for name in &self.current_path {
            if let Some(next) = curr.children.get(name) {
                curr = next;
            } else {
                break;
            }
        }
        
        let children = self.get_filtered_and_sorted_children(curr);
        self.cached_children = children.iter().map(|n| n.name.clone()).collect();
    }

    pub fn next(&mut self) {
        let count = self.cached_children.len();
        if count == 0 { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i >= count - 1 { 0 } else { i + 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let count = self.cached_children.len();
        if count == 0 { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i == 0 { count - 1 } else { i - 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn enter(&mut self) {
        if let Some(i) = self.list_state.selected() {
            let root = self.root.read().unwrap();
            let mut curr = &*root;
            for name in &self.current_path {
                curr = curr.children.get(name).unwrap();
            }
            
            let children = self.get_filtered_and_sorted_children(curr);
            
            if let Some(target) = children.get(i) {
                if target.is_dir && !target.children.is_empty() {
                    self.current_path.push(target.name.clone());
                    self.path_indices.push(i);
                    drop(root); // Release lock before update_cache
                    self.update_cache();
                    self.list_state.select(Some(0));
                }
            }
        }
    }

    pub fn back(&mut self) {
        if !self.current_path.is_empty() {
            self.current_path.pop();
            self.update_cache();
            let last_idx = self.path_indices.pop().unwrap_or(0);
            self.list_state.select(Some(last_idx));
        }
    }
}
