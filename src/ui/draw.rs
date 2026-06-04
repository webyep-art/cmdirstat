// Copyright (c) 2024 webyep
// Licensed under the MIT License.

use crate::core::app::{App, AppState, BrowsingState};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Clear},
    Frame,
};
use human_bytes::human_bytes;

pub fn draw(f: &mut Frame, app: &mut App) {
    match &mut app.state {
        AppState::Selecting(state, drives) => {
            let area = centered_rect(60, 40, f.size());
            let items: Vec<ListItem> = drives
                .iter()
                .map(|p| ListItem::new(p.to_string_lossy().to_string()))
                .collect();
            
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" Select Drive to Analyze "))
                .highlight_style(Style::default().bg(Color::Blue).add_modifier(Modifier::BOLD))
                .highlight_symbol("> ");
            
            f.render_widget(Clear, area);
            f.render_stateful_widget(list, area, state);
        }
        AppState::Browsing(s) => draw_browsing(f, s),
    }
}

fn draw_browsing(f: &mut Frame, s: &mut BrowsingState) {
    let is_scanning = s.is_scanning.load(std::sync::atomic::Ordering::Relaxed);
    if is_scanning {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.size());

        let elapsed = s.start_time.elapsed();
        let items_scanned = s.items_scanned.load(std::sync::atomic::Ordering::Relaxed);
        let skipped_files = s.skipped_files.load(std::sync::atomic::Ordering::Relaxed);
        let items_per_sec = if elapsed.as_secs() > 0 { items_scanned / elapsed.as_secs() } else { 0 };

        let elapsed_secs = elapsed.as_secs();
        let elapsed_time = format!("{:02}:{:02}", elapsed_secs / 60, elapsed_secs % 60);

        let root_locked = s.root.read().unwrap();
        let path_str = root_locked.path.display().to_string();
        drop(root_locked);

        let progress_rect = centered_rect(70, 60, chunks[0]);

        let mut progress_text = vec![
            ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Scanning: ", Style::default().add_modifier(Modifier::BOLD)),
                ratatui::text::Span::styled(path_str, Style::default().fg(Color::Cyan)),
            ]),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from(vec![
                ratatui::text::Span::raw("Items found:         "),
                ratatui::text::Span::styled(format!("{}", items_scanned), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            ratatui::text::Line::from(vec![
                ratatui::text::Span::raw("Skipped (no access): "),
                ratatui::text::Span::styled(format!("{}", skipped_files), Style::default().fg(Color::LightRed)),
            ]),
            ratatui::text::Line::from(vec![
                ratatui::text::Span::raw("Scan speed:          "),
                ratatui::text::Span::styled(format!("{} items/s", items_per_sec), Style::default().fg(Color::Green)),
            ]),
            ratatui::text::Line::from(vec![
                ratatui::text::Span::raw("Time elapsed:        "),
                ratatui::text::Span::styled(elapsed_time, Style::default().fg(Color::White)),
            ]),
        ];

        if let Some(is_ssd) = s.is_ssd {
            progress_text.push(ratatui::text::Line::from(""));
            let drive_type = if is_ssd { "SSD (Solid State)" } else { "HDD (Rotational)" };
            let drive_color = if is_ssd { Color::Green } else { Color::Yellow };
            progress_text.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Drive type:          ", Style::default().add_modifier(Modifier::BOLD)),
                ratatui::text::Span::styled(drive_type, Style::default().fg(drive_color)),
            ]));
        }

        let paragraph = Paragraph::new(progress_text)
            .block(Block::default().borders(Borders::ALL).title(" Scanning Drive / Directory "))
            .alignment(ratatui::layout::Alignment::Left);

        f.render_widget(Clear, progress_rect);
        f.render_widget(paragraph, progress_rect);

        let footer = Paragraph::new(" [q] Cancel / Quit")
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(footer, chunks[1]);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.size());

        let root_locked = s.root.read().unwrap();
        let mut current_node = &*root_locked;
        for name in &s.current_path {
            current_node = current_node.children.get(name).unwrap();
        }

        let mut header_text = vec![
            ratatui::text::Line::from(vec![
                ratatui::text::Span::raw(" Path: "),
                ratatui::text::Span::styled(current_node.path.display().to_string(), Style::default().fg(Color::Cyan)),
            ])
        ];

        let skipped = s.skipped_files.load(std::sync::atomic::Ordering::Relaxed);
        let skipped_suffix = if skipped > 0 {
            format!(" (Skipped {} unreadable items due to permissions)", skipped)
        } else {
            String::new()
        };

        header_text.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(" [SCAN COMPLETE] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ratatui::text::Span::raw(format!("Total size: {}{}", human_bytes(root_locked.size as f64), skipped_suffix)),
        ]));

        let header = Paragraph::new(header_text)
            .block(Block::default().borders(Borders::ALL).title(format!(" cmdirstat v{} ", env!("CARGO_PKG_VERSION"))));
        f.render_widget(header, chunks[0]);

        // Layout inner contents inside a block
        let contents_block = Block::default().borders(Borders::ALL).title(" Contents ");
        let inner_area = contents_block.inner(chunks[1]);
        
        let contents_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Headers
                Constraint::Length(1), // Divider line
                Constraint::Min(0),    // List
            ])
            .split(inner_area);

        let list_width = inner_area.width as usize;
        let show_accessed = list_width > 115;
        let show_modified = list_width > 90;
        let show_bar = list_width > 70;
        
        let size_w = 12;
        let bar_w = 17; // e.g. "[■■■░░░░░░░]  30%"
        let date_w = 16;
        let spacing = 2;
        
        let mut reserved_width = size_w;
        if show_bar {
            reserved_width += spacing + bar_w;
        }
        if show_modified {
            reserved_width += spacing + date_w;
        }
        if show_accessed {
            reserved_width += spacing + date_w;
        }
        
        let name_max_width = if list_width > reserved_width + 10 {
            list_width - reserved_width - 6
        } else {
            10
        };

        // Render Column Headers
        use crate::core::app::{SortColumn, SortDirection};
        let get_header_label = |col: SortColumn, label: &str, current_col: SortColumn, direction: SortDirection| -> String {
            if col == current_col {
                let arrow = if direction == SortDirection::Ascending { "▲" } else { "▼" };
                format!("{} {}", label, arrow)
            } else {
                label.to_string()
            }
        };

        let name_hdr = get_header_label(SortColumn::Name, "Name", s.sort_column, s.sort_direction);
        let size_hdr = get_header_label(SortColumn::Size, "Size", s.sort_column, s.sort_direction);
        let mod_hdr = get_header_label(SortColumn::Modified, "Modified", s.sort_column, s.sort_direction);
        let acc_hdr = get_header_label(SortColumn::Accessed, "Accessed", s.sort_column, s.sort_direction);

        let name_hdr_len = name_hdr.chars().count();
        let pad_width = name_max_width.saturating_add(4).saturating_sub(name_hdr_len);
        let name_pad = " ".repeat(pad_width);

        let mut header_spans = vec![
            ratatui::text::Span::styled(format!("{}{}", name_hdr, name_pad), Style::default().add_modifier(Modifier::BOLD)),
            ratatui::text::Span::styled(format!("{:>12}", size_hdr), Style::default().add_modifier(Modifier::BOLD)),
        ];

        if show_bar {
            header_spans.push(ratatui::text::Span::raw("  "));
            header_spans.push(ratatui::text::Span::styled(format!("{:>17}", "Usage"), Style::default().add_modifier(Modifier::BOLD)));
        }
        if show_modified {
            header_spans.push(ratatui::text::Span::raw("  "));
            header_spans.push(ratatui::text::Span::styled(format!("{:>16}", mod_hdr), Style::default().add_modifier(Modifier::BOLD)));
        }
        if show_accessed {
            header_spans.push(ratatui::text::Span::raw("  "));
            header_spans.push(ratatui::text::Span::styled(format!("{:>16}", acc_hdr), Style::default().add_modifier(Modifier::BOLD)));
        }

        let headers_paragraph = Paragraph::new(ratatui::text::Line::from(header_spans));
        f.render_widget(headers_paragraph, contents_layout[0]);

        // Divider
        let divider_line = "─".repeat(list_width);
        let divider_paragraph = Paragraph::new(divider_line).style(Style::default().fg(Color::DarkGray));
        f.render_widget(divider_paragraph, contents_layout[1]);

        // Fetch children
        let children = s.get_filtered_and_sorted_children(current_node);

        let total_size = current_node.size;

        let items: Vec<ListItem> = children
            .iter()
            .map(|node| {
                let prefix = if node.is_dir { "► " } else { "  " };
                let size_str = human_bytes(node.size as f64);
                let mod_str = format_system_time(node.modified);
                let acc_str = format_system_time(node.accessed);
                
                // Truncation
                let name = node.name.clone();
                let display_name = format!("{}{}", prefix, name);
                let display_name_len = unicode_width::UnicodeWidthStr::width(display_name.as_str());
                
                let mut truncated_name = display_name;
                if display_name_len > name_max_width {
                    let mut name_part = name.chars().take(name_max_width.saturating_sub(5)).collect::<String>();
                    name_part.push_str("...");
                    truncated_name = format!("{}{}", prefix, name_part);
                }
                
                let actual_len = unicode_width::UnicodeWidthStr::width(truncated_name.as_str());
                let pad_width = name_max_width.saturating_add(4).saturating_sub(actual_len);
                let padding = " ".repeat(pad_width);
                
                let mut line_spans = vec![
                    ratatui::text::Span::raw(format!("{}{}", truncated_name, padding)),
                    ratatui::text::Span::raw(format!("{:>12}", size_str)),
                ];

                if show_bar {
                    let percent = if total_size > 0 { (node.size as f64 / total_size as f64) * 100.0 } else { 0.0 };
                    let bar_slots = 10;
                    let filled = ((percent / 100.0) * bar_slots as f64).round() as usize;
                    let bar_str = format!(
                        "[{}{}] {:>3.0}%",
                        "■".repeat(filled),
                        "░".repeat(bar_slots - filled),
                        percent
                    );
                    line_spans.push(ratatui::text::Span::raw("  "));
                    line_spans.push(ratatui::text::Span::raw(format!("{:>17}", bar_str)));
                }
                
                if show_modified {
                    line_spans.push(ratatui::text::Span::raw("  "));
                    line_spans.push(ratatui::text::Span::raw(format!("{:>16}", mod_str)));
                }
                if show_accessed {
                    line_spans.push(ratatui::text::Span::raw("  "));
                    line_spans.push(ratatui::text::Span::raw(format!("{:>16}", acc_str)));
                }
                
                ListItem::new(ratatui::text::Line::from(line_spans))
                    .style(Style::default().fg(if node.is_dir { Color::Cyan } else { Color::White }))
            })
            .collect();
        drop(root_locked); // Release lock

        let list = List::new(items)
            .highlight_style(Style::default().bg(Color::Indexed(237)).add_modifier(Modifier::BOLD))
            .highlight_symbol(" ");

        f.render_widget(contents_block, chunks[1]);
        f.render_stateful_widget(list, contents_layout[2], &mut s.list_state);

        // Footer / Search
        let footer_text = if s.search_active {
            format!(" SEARCH: {}_ (Press Enter to apply, Esc to clear)", s.search_query)
        } else if !s.search_query.is_empty() {
            format!(" FILTER ACTIVE: {} (Press / to edit, Esc to clear) | [Enter/l/→] Enter  [BS/h/←] Back  [q] Quit", s.search_query)
        } else {
            " [Enter/l/→] Enter  [BS/h/←] Back  [d/Del] DELETE  [Tab/s] Column  [Space/r] Order  [?/F1] Help  [q] Quit".to_string()
        };

        let footer = Paragraph::new(footer_text)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(footer, chunks[2]);

        // Confirmation popup
        if let Some(del_item) = &s.deleting_item {
            let popup_area = centered_rect(65, 30, f.size());
            let popup_text = vec![
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw("Are you sure you want to permanently delete:"),
                ]),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(format!("  {}", del_item.name), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                ]),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("WARNING: This action is permanent and CANNOT be undone.", Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)),
                ]),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(" [y] Yes, Delete permanently ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("   "),
                    ratatui::text::Span::styled(" [n/Esc] Cancel ", Style::default().fg(Color::Yellow)),
                ]),
            ];

            let popup_paragraph = Paragraph::new(popup_text)
                .block(Block::default().borders(Borders::ALL).title(" Confirm Permanent Deletion "))
                .alignment(ratatui::layout::Alignment::Center);

            f.render_widget(Clear, popup_area);
            f.render_widget(popup_paragraph, popup_area);
        }

        // Deletion Error popup
        if let Some(err_msg) = &s.deletion_error {
            let popup_area = centered_rect(60, 25, f.size());
            let popup_text = vec![
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(" ERROR DELETING ITEM ", Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)),
                ]),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw(err_msg),
                ]),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(" Press any key to dismiss ", Style::default().fg(Color::DarkGray)),
                ]),
            ];
            let popup_paragraph = Paragraph::new(popup_text)
                .block(Block::default().borders(Borders::ALL).title(" Error "))
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(Clear, popup_area);
            f.render_widget(popup_paragraph, popup_area);
        }

        // Help Modal Popup
        if s.show_help {
            let popup_area = centered_rect(65, 48, f.size());
            let help_text = vec![
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(" KEYBOARD SHORTCUTS ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                ]),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("  Navigate:   ", Style::default().add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("j/k/Down/Up"),
                ]),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("  Enter:      ", Style::default().add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("Enter/l/Right"),
                ]),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("  Back:       ", Style::default().add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("Backspace/h/Left"),
                ]),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("  Delete:     ", Style::default().add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("d/Delete (shows confirmation)"),
                ]),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("  Sort Col:   ", Style::default().add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("Tab/s (cycles Name/Size/Dates)"),
                ]),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("  Sort Order: ", Style::default().add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("Space/r (toggle Asc/Desc)"),
                ]),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("  Search:     ", Style::default().add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("/ (type search, Esc clears filter)"),
                ]),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("  Help:       ", Style::default().add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("?/F1 (toggle help screen)"),
                ]),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("  Quit:       ", Style::default().add_modifier(Modifier::BOLD)),
                    ratatui::text::Span::raw("q"),
                ]),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(" Press Esc, ?, or F1 to dismiss help ", Style::default().fg(Color::DarkGray)),
                ]),
            ];
            let popup_paragraph = Paragraph::new(help_text)
                .block(Block::default().borders(Borders::ALL).title(" cmdirstat Help "))
                .alignment(ratatui::layout::Alignment::Left);
            f.render_widget(Clear, popup_area);
            f.render_widget(popup_paragraph, popup_area);
        }
    }
}

fn format_system_time(time: std::time::SystemTime) -> String {
    if time == std::time::SystemTime::UNIX_EPOCH {
        return "N/A".to_string();
    }
    let datetime: chrono::DateTime<chrono::Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
