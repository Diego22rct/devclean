use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState};
use ratatui::Frame;

pub use crate::app::format_bytes;
use crate::app::{App, ModalState};
use crate::ecosystem::Ecosystem;

/// Returns the characteristic accent color style for a development ecosystem tag.
fn ecosystem_style(ecosystem: Ecosystem) -> Style {
    match ecosystem {
        Ecosystem::Node => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        Ecosystem::Flutter => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        Ecosystem::Rust => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
        Ecosystem::DotNet => Style::default()
            .fg(Color::LightBlue)
            .add_modifier(Modifier::BOLD),
        Ecosystem::Python => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    }
}

/// Computes a centered rectangle within the given parent area.
fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    let w = width.min(r.width.saturating_sub(2)).max(20);
    let h = height.min(r.height.saturating_sub(2)).max(5);
    let x = r.x + (r.width.saturating_sub(w)) / 2;
    let y = r.y + (r.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

/// Renders the entire terminal user interface onto the ratatui frame.
pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Top summary header
            Constraint::Min(5),    // Main artifacts table
            Constraint::Length(4), // Footer keybindings & status
        ])
        .split(frame.area());

    // 1. Top Header Block
    let mode_badge = if app.dry_run {
        Span::styled(
            "[SIMULATION / DRY-RUN]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "[LIVE]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    };

    let scan_status = if !app.scanning_done {
        Span::styled(
            " [Scanning workspace...]",
            Style::default().fg(Color::Yellow),
        )
    } else {
        Span::styled(" [Scan complete]", Style::default().fg(Color::DarkGray))
    };

    let title_line = Line::from(vec![
        Span::styled(
            " devclean ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("— Multi-ecosystem project artifact cleaner  "),
        mode_badge,
        scan_status,
    ]);

    let mut summary_spans = vec![
        Span::styled("Root: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            app.root_path.display().to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  │  "),
        Span::styled("Items: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            app.total_items().to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  │  "),
        Span::styled("Reclaimable: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format_bytes(app.total_bytes()),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if app.total_freed_bytes > 0 {
        summary_spans.push(Span::raw("  │  "));
        summary_spans.push(Span::styled(
            "Freed: ",
            Style::default().fg(Color::DarkGray),
        ));
        summary_spans.push(Span::styled(
            format_bytes(app.total_freed_bytes),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let summary_line = Line::from(summary_spans);

    let header_widget = Paragraph::new(vec![title_line, summary_line]).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(header_widget, chunks[0]);

    // 2. Main Artifacts Table
    let header_cells = [
        "Sel",
        "Ecosystem",
        "Project",
        "Artifact Path",
        "Size",
        "Last Modified",
    ]
    .iter()
    .map(|h| {
        Cell::from(*h).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    });
    let header_row = Row::new(header_cells)
        .style(Style::default().bg(Color::Rgb(30, 30, 40)))
        .height(1);

    let rows: Vec<Row> = if app.items.is_empty() {
        if app.scanning_done {
            vec![Row::new(vec![
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
                Cell::from("No regenerable build artifacts discovered."),
                Cell::from(""),
                Cell::from(""),
            ])
            .style(Style::default().fg(Color::DarkGray))]
        } else {
            vec![Row::new(vec![
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
                Cell::from("Scanning directories for build artifacts..."),
                Cell::from(""),
                Cell::from(""),
            ])
            .style(Style::default().fg(Color::Yellow))]
        }
    } else {
        app.items
            .iter()
            .map(|item| {
                let checkbox = if item.selected {
                    Cell::from("[x]").style(
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Cell::from("[ ]").style(Style::default().fg(Color::DarkGray))
                };

                let eco_badge =
                    Cell::from(item.ecosystem.tag()).style(ecosystem_style(item.ecosystem));
                let project =
                    Cell::from(item.project_name.as_str()).style(Style::default().fg(Color::White));

                let relative_path = item
                    .path
                    .strip_prefix(&app.root_path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| item.path.display().to_string());
                let path_cell = Cell::from(relative_path);

                let size_cell = match item.size_bytes {
                    Some(bytes) => {
                        Cell::from(format_bytes(bytes)).style(Style::default().fg(Color::White))
                    }
                    None => {
                        Cell::from("calculating...").style(Style::default().fg(Color::DarkGray))
                    }
                };

                let date_cell =
                    Cell::from(item.formatted_date()).style(Style::default().fg(Color::DarkGray));

                Row::new(vec![
                    checkbox, eco_badge, project, path_cell, size_cell, date_cell,
                ])
            })
            .collect()
    };

    let widths = [
        Constraint::Length(5),      // Sel
        Constraint::Length(11),     // Ecosystem
        Constraint::Length(18),     // Project
        Constraint::Percentage(45), // Artifact Path
        Constraint::Length(14),     // Size
        Constraint::Length(18),     // Last Modified
    ];

    let mut table_state = TableState::default();
    if !app.items.is_empty() {
        table_state.select(Some(app.cursor));
    }

    let table = Table::new(rows, widths)
        .header(header_row)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Discovered Artifacts "),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(table, chunks[1], &mut table_state);

    // 3. Footer Keybindings & Status Block
    let keybindings_line = Line::from(vec![
        Span::styled(
            "[↑/↓/j/k]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Navigate  "),
        Span::styled(
            "[Space]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Select  "),
        Span::styled(
            "[a]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" All  "),
        Span::styled(
            "[s]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Sort  "),
        Span::styled(
            "[d]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Delete  "),
        Span::styled(
            "[q]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Quit"),
    ]);

    let mut status_spans = vec![
        Span::styled("Sort: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            app.sort_mode.label(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  │  "),
        Span::styled("Selected: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(
                "{} items ({})",
                app.selected_count(),
                format_bytes(app.selected_bytes())
            ),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if let Some(msg) = &app.status_message {
        status_spans.push(Span::raw("  │  "));
        status_spans.push(Span::styled(
            msg.as_str(),
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let status_line = Line::from(status_spans);

    let footer_widget = Paragraph::new(vec![keybindings_line, status_line]).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(footer_widget, chunks[2]);

    // 4. Modal Overlays
    match &app.modal {
        ModalState::ConfirmDelete { count, total_bytes } => {
            let area = centered_rect(64, 10, frame.area());
            frame.render_widget(Clear, area);

            let border_color = if app.dry_run {
                Color::Yellow
            } else {
                Color::Red
            };
            let mode_str = if app.dry_run {
                "[SIMULATION / DRY-RUN] (No files will be modified)"
            } else {
                "[PERMANENT DELETION] (Files will be PERMANENTLY deleted)"
            };

            let lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw("Delete "),
                    Span::styled(
                        count.to_string(),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" artifact directory(s)?"),
                ]),
                Line::from(vec![
                    Span::raw("Total space to reclaim: "),
                    Span::styled(
                        format_bytes(*total_bytes),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("Mode: "),
                    Span::styled(
                        mode_str,
                        Style::default()
                            .fg(border_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "[Y] Confirm Deletion",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("      "),
                    Span::styled(
                        "[N / Esc] Cancel",
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            ];

            let popup = Paragraph::new(lines).alignment(Alignment::Center).block(
                Block::default()
                    .title(" Confirm Deletion ")
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(border_color)
                            .add_modifier(Modifier::BOLD),
                    ),
            );

            frame.render_widget(popup, area);
        }
        ModalState::Deleting {
            completed,
            total,
            current_item,
        } => {
            let area = centered_rect(68, 9, frame.area());
            frame.render_widget(Clear, area);

            let border_color = if app.dry_run {
                Color::Yellow
            } else {
                Color::Cyan
            };

            let percent = (*completed * 100).checked_div(*total).unwrap_or(0);
            let gauge_width = 30usize;
            let filled = (*completed * gauge_width).checked_div(*total).unwrap_or(0);
            let bar = format!(
                "[{}{}] {:>3}%",
                "=".repeat(filled),
                " ".repeat(gauge_width.saturating_sub(filled)),
                percent
            );

            let lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        format!("Deleting [{}/{}]: ", completed, total),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        current_item.as_str(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
                Line::from(Span::styled(
                    bar,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Please wait while artifacts are deleted...",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let title = if app.dry_run {
                " Simulating Deletion "
            } else {
                " Deleting Artifacts "
            };

            let popup = Paragraph::new(lines).alignment(Alignment::Center).block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(border_color)
                            .add_modifier(Modifier::BOLD),
                    ),
            );

            frame.render_widget(popup, area);
        }
        ModalState::Message { title, text } => {
            let line_count = text.lines().count();
            let max_line_len = text.lines().map(|l| l.len()).max().unwrap_or(0);
            let width = ((max_line_len as u16 + 8).max(64)).min(frame.area().width.saturating_sub(2));
            let height = ((line_count as u16 + 6).max(8)).min(frame.area().height.saturating_sub(2));

            let area = centered_rect(width, height, frame.area());
            frame.render_widget(Clear, area);

            let mut lines = vec![Line::from("")];
            lines.extend(text.lines().map(|l| {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(Color::White),
                ))
            }));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "[Enter / Esc / Space] Close",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));

            let popup = Paragraph::new(lines).alignment(Alignment::Center).block(
                Block::default()
                    .title(format!(" {} ", title))
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
            );

            frame.render_widget(popup, area);
        }
        ModalState::None => {}
    }
}
