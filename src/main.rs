// Copyright (c) 2024 webyep
// Licensed under the MIT License.

mod core;
mod ui;

use crate::core::app::{App, AppState};
use crate::core::scanner::Scanner;
use crate::core::selector::get_available_drives;
use crate::ui::tui::Tui;
use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::path::PathBuf;
use std::time::Duration;
use std::thread;

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Limit the number of threads (controls memory and CPU usage)
    #[arg(short, long)]
    threads: Option<usize>,
}

pub enum ScanMessage {
    Update(crate::core::tree::Node),
    Done(crate::core::tree::Node),
    Error(String),
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(t) = args.threads {
        rayon::ThreadPoolBuilder::new().num_threads(t).build_global()?;
    }

    let mut tui = Tui::new()?;
    
    let mut app = if let Some(path) = args.path {
        App::new_selecting(vec![path])
    } else {
        App::new_selecting(get_available_drives())
    };

    run_app(&mut tui, &mut app)?;

    Ok(())
}

fn run_app(tui: &mut Tui, app: &mut App) -> Result<()> {
    let mut last_draw = std::time::Instant::now();
    let mut needs_draw = true;
    let mut was_scanning = false;

    loop {
        let is_scanning = match &app.state {
            AppState::Browsing(s) => s.is_scanning.load(std::sync::atomic::Ordering::Relaxed),
            _ => false,
        };

        if needs_draw || (is_scanning && last_draw.elapsed() >= Duration::from_millis(100)) {
            tui.terminal.draw(|f| ui::draw::draw(f, app))?;
            last_draw = std::time::Instant::now();
            needs_draw = false;
        }

        let poll_duration = if is_scanning {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(250)
        };

        if event::poll(poll_duration)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    if let AppState::Browsing(s) = &mut app.state {
                        if s.deletion_error.is_some() {
                            s.deletion_error = None;
                            needs_draw = true;
                            continue;
                        }

                        if s.show_help {
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => {
                                    s.show_help = false;
                                    needs_draw = true;
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if s.search_active {
                            match key.code {
                                KeyCode::Esc => {
                                    s.search_active = false;
                                    s.search_query.clear();
                                    s.update_cache();
                                    needs_draw = true;
                                }
                                KeyCode::Enter => {
                                    s.search_active = false;
                                    needs_draw = true;
                                }
                                KeyCode::Backspace => {
                                    s.search_query.pop();
                                    s.update_cache();
                                    needs_draw = true;
                                }
                                KeyCode::Char(c) => {
                                    s.search_query.push(c);
                                    s.update_cache();
                                    needs_draw = true;
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if let Some(del_item) = s.deleting_item.clone() {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                                    let res = if del_item.path.is_dir() {
                                        std::fs::remove_dir_all(&del_item.path)
                                    } else {
                                        std::fs::remove_file(&del_item.path)
                                    };

                                    match res {
                                        Ok(_) => {
                                            {
                                                let mut root = s.root.write().unwrap();
                                                let mut curr = &mut *root;
                                                for name in &s.current_path {
                                                    curr = curr.children.get_mut(name).unwrap();
                                                }
                                                curr.children.remove(&del_item.name);
                                            }
                                            s.update_cache();
                                            s.deleting_item = None;
                                        }
                                        Err(err) => {
                                            s.deleting_item = None;
                                            s.deletion_error = Some(err.to_string());
                                        }
                                    }
                                    needs_draw = true;
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    s.deleting_item = None;
                                    needs_draw = true;
                                }
                                _ => {}
                            }
                            continue;
                        }
                    }

                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('?') | KeyCode::F(1) => {
                            if let AppState::Browsing(s) = &mut app.state {
                                s.show_help = true;
                                needs_draw = true;
                            }
                        }
                        KeyCode::Char('/') => {
                            if let AppState::Browsing(s) = &mut app.state {
                                if !s.is_scanning.load(std::sync::atomic::Ordering::Relaxed) {
                                    s.search_active = true;
                                    s.search_query.clear();
                                    needs_draw = true;
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.next();
                            needs_draw = true;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.previous();
                            needs_draw = true;
                        }
                        KeyCode::Char('d') | KeyCode::Delete => {
                            if let AppState::Browsing(s) = &mut app.state {
                                if s.is_scanning.load(std::sync::atomic::Ordering::Relaxed) {
                                    continue;
                                }
                                if let Some(i) = s.list_state.selected() {
                                    let (path_to_del, name_to_del) = {
                                        let root = s.root.read().unwrap();
                                        let mut curr = &*root;
                                        for name in &s.current_path {
                                            curr = curr.children.get(name).unwrap();
                                        }
                                        let children = s.get_filtered_and_sorted_children(curr);
                                        if let Some(node) = children.get(i) {
                                            (node.path.clone(), node.name.clone())
                                        } else {
                                            continue;
                                        }
                                    };
                                    s.deleting_item = Some(crate::core::app::DeletingItem {
                                        path: path_to_del,
                                        name: name_to_del,
                                    });
                                    needs_draw = true;
                                }
                            }
                        }
                        KeyCode::Char('s') | KeyCode::Tab => {
                            if let AppState::Browsing(s) = &mut app.state {
                                if !s.is_scanning.load(std::sync::atomic::Ordering::Relaxed) {
                                    s.cycle_sort_column();
                                    needs_draw = true;
                                }
                            }
                        }
                        KeyCode::Char('r') | KeyCode::Char(' ') => {
                            if let AppState::Browsing(s) = &mut app.state {
                                if !s.is_scanning.load(std::sync::atomic::Ordering::Relaxed) {
                                    s.toggle_sort_direction();
                                    needs_draw = true;
                                }
                            }
                        }
                        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                            if let Some(path) = app.enter() {
                                if let AppState::Selecting(_, _) = &app.state {
                                    app.start_browsing(path.clone());
                                    if let AppState::Browsing(s) = &app.state {
                                        let root = s.root.clone();
                                        let is_scanning = s.is_scanning.clone();
                                        let items_scanned = s.items_scanned.clone();
                                        let skipped_files = s.skipped_files.clone();
                                        thread::spawn(move || {
                                            let _ = Scanner::scan_incremental(path, root, is_scanning, items_scanned, skipped_files);
                                        });
                                    }
                                }
                            }
                            needs_draw = true;
                        }
                        KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                            app.back();
                            needs_draw = true;
                        }
                        _ => {}
                    }
                }
                Event::Resize(_, _) => {
                    needs_draw = true;
                }
                _ => {}
            }
        }
        
        // Handle transitions and cache updates
        if let AppState::Browsing(s) = &mut app.state {
            let is_scanning = s.is_scanning.load(std::sync::atomic::Ordering::Relaxed);
            if is_scanning {
                was_scanning = true;
            } else if was_scanning {
                s.update_cache();
                was_scanning = false;
                needs_draw = true;
            }
        }
    }
}
