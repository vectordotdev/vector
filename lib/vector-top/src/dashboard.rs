use std::{io::stdout, time::Duration};

use crossterm::{
    ExecutableCommand,
    cursor::Show,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    tty::IsTty,
};
use num_format::{Locale, ToFormattedString};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Flex, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, ListState, Padding, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
};
use tokio::sync::oneshot;
use unit_prefix::NumberPrefix;

use crate::{
    input::{InputMode, handle_input},
    state::{ComponentRow, FilterColumn, FilterMenuState, SortColumn},
};

use super::{
    events::capture_key_press,
    state::{self, ConnectionStatus},
};

pub const fn is_allocation_tracing_enabled() -> bool {
    cfg!(feature = "allocation-tracing")
}

/// Format metrics, with thousands separation
trait ThousandsFormatter {
    fn thousands_format(&self) -> String;
}

impl ThousandsFormatter for u32 {
    fn thousands_format(&self) -> String {
        match self {
            0 => "--".into(),
            _ => self.to_formatted_string(&Locale::en),
        }
    }
}

impl ThousandsFormatter for u64 {
    fn thousands_format(&self) -> String {
        match self {
            0 => "--".into(),
            _ => self.to_formatted_string(&Locale::en),
        }
    }
}

impl ThousandsFormatter for i64 {
    fn thousands_format(&self) -> String {
        match self {
            0 => "--".into(),
            _ => self.to_formatted_string(&Locale::en),
        }
    }
}

/// Format metrics, using the 'humanized' format, abbreviating with suffixes
trait HumanFormatter {
    fn human_format(&self) -> String;
    fn human_format_bytes(&self) -> String;
}

impl HumanFormatter for i64 {
    /// Format an i64 as a string, returning `--` if zero, the value as a string if < 1000, or
    /// the value and the recognised abbreviation
    fn human_format(&self) -> String {
        match self {
            0 => "--".into(),
            n => match NumberPrefix::decimal(*n as f64) {
                NumberPrefix::Standalone(n) => n.to_string(),
                NumberPrefix::Prefixed(p, n) => format!("{n:.2} {p}"),
            },
        }
    }

    /// Format an i64 as a string in the same way as `human_format`, but using a 1024 base
    /// for binary, and appended with a "B" to represent byte values
    fn human_format_bytes(&self) -> String {
        match self {
            0 => "--".into(),
            n => match NumberPrefix::binary(*n as f64) {
                NumberPrefix::Standalone(n) => n.to_string(),
                NumberPrefix::Prefixed(p, n) => format!("{n:.2} {p}B"),
            },
        }
    }
}

fn format_metric(total: i64, throughput: i64, human_metrics: bool) -> String {
    match total {
        0 => "N/A".to_string(),
        v => format!(
            "{} ({}/s)",
            if human_metrics {
                v.human_format()
            } else {
                v.thousands_format()
            },
            throughput.human_format()
        ),
    }
}

fn format_metric_bytes(total: i64, throughput: i64, human_metrics: bool) -> String {
    match total {
        0 => "N/A".to_string(),
        v => format!(
            "{} ({}/s)",
            if human_metrics {
                v.human_format_bytes()
            } else {
                v.thousands_format()
            },
            throughput.human_format_bytes()
        ),
    }
}

const NUM_COLUMNS: usize = if is_allocation_tracing_enabled() {
    10
} else {
    9
};

static HEADER: [&str; NUM_COLUMNS] = [
    "ID",
    "Output",
    "Kind",
    "Type",
    "Events In",
    "Bytes In",
    "Events Out",
    "Bytes Out",
    "Errors",
    #[cfg(feature = "allocation-tracing")]
    "Memory Used",
];

struct Widgets<'a> {
    constraints: Vec<Constraint>,
    url_string: &'a str,
    interval: u32,
    human_metrics: bool,
    title: &'a str,
}

impl<'a> Widgets<'a> {
    /// Creates a new Widgets, containing constraints to re-use across renders.
    pub fn new(title: &'a str, url_string: &'a str, interval: u32, human_metrics: bool) -> Self {
        let constraints = vec![
            Constraint::Length(3),
            Constraint::Max(90),
            Constraint::Length(3),
        ];

        Self {
            constraints,
            url_string,
            interval,
            human_metrics,
            title,
        }
    }

    /// Renders a title and the URL the dashboard is currently connected to.
    fn title(
        &'a self,
        f: &mut Frame,
        area: Rect,
        connection_status: &ConnectionStatus,
        uptime: Duration,
    ) {
        let mut text = vec![
            Span::from(self.url_string),
            Span::styled(
                format!(" | Sampling @ {}ms", self.interval.thousands_format()),
                Style::default().fg(Color::Gray),
            ),
            Span::from(" | "),
        ];
        text.extend(connection_status.as_ui_spans());
        text.extend(vec![Span::from(format!(
            " | Uptime: {}",
            humantime::format_duration(uptime)
        ))]);

        let text = vec![Line::from(text)];

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            self.title,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
        let w = Paragraph::new(text).block(block).wrap(Wrap { trim: true });

        f.render_widget(w, area);
    }

    /// Renders a components table, showing sources, transforms and sinks in tabular form, with
    /// statistics pulled from `ComponentsState`,
    fn components_table(&self, f: &mut Frame, state: &state::State, area: Rect) {
        // Header columns
        let header = HEADER
            .iter()
            .map(|s| {
                let mut c = Cell::from(*s).style(Style::default().add_modifier(Modifier::BOLD));
                if state
                    .sort_state
                    .column
                    .map(|c| c.matches_header(s))
                    .unwrap_or_default()
                {
                    c = c.add_modifier(Modifier::REVERSED);
                }
                c
            })
            .collect::<Vec<_>>();

        // Data columns
        let mut items = Vec::new();
        let mut sorted = state.components.iter().collect::<Vec<_>>();
        if let Some(column) = state.sort_state.column {
            let sort_fn = match column {
                SortColumn::Id => |l: &ComponentRow, r: &ComponentRow| l.key.cmp(&r.key),
                SortColumn::Kind => |l: &ComponentRow, r: &ComponentRow| l.kind.cmp(&r.kind),
                SortColumn::Type => {
                    |l: &ComponentRow, r: &ComponentRow| l.component_type.cmp(&r.component_type)
                }
                SortColumn::EventsIn => |l: &ComponentRow, r: &ComponentRow| {
                    l.received_events_throughput_sec
                        .cmp(&r.received_events_throughput_sec)
                },
                SortColumn::EventsInTotal => |l: &ComponentRow, r: &ComponentRow| {
                    l.received_events_total.cmp(&r.received_events_total)
                },
                SortColumn::BytesIn => |l: &ComponentRow, r: &ComponentRow| {
                    l.received_bytes_throughput_sec
                        .cmp(&r.received_bytes_throughput_sec)
                },
                SortColumn::BytesInTotal => |l: &ComponentRow, r: &ComponentRow| {
                    l.received_bytes_total.cmp(&r.received_bytes_total)
                },
                SortColumn::EventsOut => |l: &ComponentRow, r: &ComponentRow| {
                    l.sent_events_throughput_sec
                        .cmp(&r.sent_events_throughput_sec)
                },
                SortColumn::EventsOutTotal => |l: &ComponentRow, r: &ComponentRow| {
                    l.sent_events_total.cmp(&r.sent_events_total)
                },
                SortColumn::BytesOut => |l: &ComponentRow, r: &ComponentRow| {
                    l.sent_bytes_throughput_sec
                        .cmp(&r.sent_bytes_throughput_sec)
                },
                SortColumn::BytesOutTotal => {
                    |l: &ComponentRow, r: &ComponentRow| l.sent_bytes_total.cmp(&r.sent_bytes_total)
                }
                SortColumn::Errors => |l: &ComponentRow, r: &ComponentRow| l.errors.cmp(&r.errors),
                #[cfg(feature = "allocation-tracing")]
                SortColumn::MemoryUsed => {
                    |l: &ComponentRow, r: &ComponentRow| l.allocated_bytes.cmp(&r.allocated_bytes)
                }
            };
            if state.sort_state.reverse {
                sorted.sort_by(|a, b| sort_fn(a.1, b.1).reverse())
            } else {
                sorted.sort_by(|a, b| sort_fn(a.1, b.1));
            }
        }

        for (_, r) in sorted.into_iter().filter(|(_, r)| {
            let column = state.filter_state.column;
            if let Some(regex) = &state.filter_state.pattern {
                match column {
                    FilterColumn::Id => {
                        regex.is_match(r.key.id()) || r.key.id().contains(regex.as_str())
                    }
                    FilterColumn::Kind => {
                        regex.is_match(&r.kind) || r.kind.contains(regex.as_str())
                    }
                    FilterColumn::Type => {
                        regex.is_match(&r.component_type)
                            || r.component_type.contains(regex.as_str())
                    }
                }
            } else {
                true
            }
        }) {
            let mut data = vec![
                r.key.id().to_string(),
                if !r.has_displayable_outputs() {
                    "--"
                } else {
                    Default::default()
                }
                .to_string(),
                r.kind.clone(),
                r.component_type.clone(),
            ];

            let formatted_metrics = [
                format_metric(
                    r.received_events_total,
                    r.received_events_throughput_sec,
                    self.human_metrics,
                ),
                format_metric_bytes(
                    r.received_bytes_total,
                    r.received_bytes_throughput_sec,
                    self.human_metrics,
                ),
                format_metric(
                    r.sent_events_total,
                    r.sent_events_throughput_sec,
                    self.human_metrics,
                ),
                format_metric_bytes(
                    r.sent_bytes_total,
                    r.sent_bytes_throughput_sec,
                    self.human_metrics,
                ),
                if self.human_metrics {
                    r.errors.human_format()
                } else {
                    r.errors.thousands_format()
                },
                #[cfg(feature = "allocation-tracing")]
                r.allocated_bytes.human_format_bytes(),
            ];

            data.extend_from_slice(&formatted_metrics);
            items.push(Row::new(data).style(Style::default()));

            // Add output rows
            if r.has_displayable_outputs() {
                for (id, output) in r.outputs.iter() {
                    let sent_events_metric = format_metric(
                        output.sent_events_total,
                        output.sent_events_throughput_sec,
                        self.human_metrics,
                    );
                    let mut data = [""; NUM_COLUMNS]
                        .into_iter()
                        .map(Cell::from)
                        .collect::<Vec<_>>();
                    data[1] = Cell::from(id.as_str());
                    data[5] = Cell::from(sent_events_metric);
                    items.push(Row::new(data).style(Style::default()));
                }
            }
        }

        let widths: &[Constraint] = if is_allocation_tracing_enabled() {
            &[
                Constraint::Percentage(13), // ID
                Constraint::Percentage(8),  // Output
                Constraint::Percentage(4),  // Kind
                Constraint::Percentage(9),  // Type
                Constraint::Percentage(10), // Events In
                Constraint::Percentage(12), // Bytes In
                Constraint::Percentage(10), // Events Out
                Constraint::Percentage(12), // Bytes Out
                Constraint::Percentage(8),  // Errors
                Constraint::Percentage(14), // Allocated Bytes
            ]
        } else {
            &[
                Constraint::Percentage(13), // ID
                Constraint::Percentage(12), // Output
                Constraint::Percentage(9),  // Kind
                Constraint::Percentage(6),  // Type
                Constraint::Percentage(12), // Events In
                Constraint::Percentage(14), // Bytes In
                Constraint::Percentage(12), // Events Out
                Constraint::Percentage(14), // Bytes Out
                Constraint::Percentage(8),  // Errors
            ]
        };
        let w = Table::new(items, widths)
            .header(Row::new(header).bottom_margin(1))
            .block(Block::default().borders(Borders::ALL).title("Components"))
            .column_spacing(2);
        f.render_stateful_widget(
            w,
            area,
            // We don't need selection, so just create a table state for the scroll
            &mut TableState::new().with_offset(state.ui.scroll),
        );
        // Skip the border + header row + 1 row of padding as well as the bottom border
        let scrollbar_area = Rect::new(area.x, area.y + 3, area.width, area.height - 4);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            scrollbar_area,
            &mut ScrollbarState::new(
                // Maximum allowed scroll value
                // We calculate it like this, because scrollbar usually accounts for full
                // overscroll, but we want scrolling to stop when last available item is visible and
                // at the bottom of the table.
                state
                    .components
                    .len()
                    .saturating_sub(scrollbar_area.height.into())
                    // 1 is also added, because ScrollBar removes 1, to ensure last item is visible
                    // when overscrolling - we avoid overscroll, so this is useless to us.
                    .saturating_add(1),
            )
            .position(state.ui.scroll),
        );
    }

    /// Alerts the user to resize the window to view columns
    fn components_resize_window(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Components");
        let w = Paragraph::new("Expand the window to > 80 chars to view metrics")
            .block(block)
            .wrap(Wrap { trim: true });

        f.render_widget(w, area);
    }

    /// Renders a box showing instructions on how to use `vector top`.
    fn help_box(&self, f: &mut Frame, area: Rect) {
        let text = vec![
            Line::from("General").bold(),
            Line::from("ESC, q => quit (or close window)"),
            Line::from("↓, j => scroll down by 1 row"),
            Line::from("↑, k => scroll up by 1 row"),
            Line::from("→, PageDown, CTRL+f => scroll down by 1 page"),
            Line::from("←, PageUp, CTRL+b => scroll up by 1 page"),
            Line::from("End, G => scroll to bottom"),
            Line::from("Home, g => scroll to top"),
            Line::from("F1, ? => toggle this help window"),
            Line::from("1-9 => sort by column"),
            Line::from("F6, s => toggle sort menu"),
            Line::from("F7, r => toggle ascending/descending sort"),
            Line::from("F4, f, / => toggle filter menu"),
            Line::default(),
            Line::from("Sort menu").bold(),
            Line::from("↑, k => move sort column selection up"),
            Line::from("↓, j => move sort column selection down"),
            Line::from("Enter => confirm sort selection"),
            Line::from("F6, s => toggle sort menu"),
            Line::default(),
            Line::from("Filter menu").bold(),
            Line::from("Shift+Tab, ↑ => move filter column selection up"),
            Line::from("Tab, ↓ => move filter column selection down"),
            Line::from("Enter => confirm filter selection"),
            Line::from("F4 => toggle sort menu"),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default())
            .padding(Padding::proportional(2))
            .title("Help");
        let w = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Left);

        f.render_widget(Clear, area);
        f.render_widget(w, area);
    }

    /// Renders a box with sorting options.
    fn sort_box(&self, f: &mut Frame, area: Rect, mut list_state: ListState) {
        f.render_widget(Clear, area);
        let w = List::new(
            SortColumn::items()
                .into_iter()
                .map(|h| ListItem::new(Line::from(h))),
        )
        .block(
            Block::default()
                .padding(Padding::proportional(2))
                .borders(Borders::ALL)
                .title("Sort by"),
        )
        .highlight_style(Style::new().reversed());
        f.render_stateful_widget(w, area, &mut list_state);
    }

    /// Renders a box with filtering options.
    fn filter_box(&self, f: &mut Frame, area: Rect, filter_menu_state: &FilterMenuState) {
        f.render_widget(Clear, area);
        let w = List::new(
            FilterColumn::items()
                .into_iter()
                .map(|h| ListItem::new(Line::from(h))),
        )
        .block(Block::default().borders(Borders::ALL).title("Filter by"))
        .highlight_style(Style::new().reversed());
        let (top, bottom) = {
            (
                Rect::new(area.x, area.y, area.width, area.height / 2),
                Rect::new(
                    area.x,
                    area.y + area.height / 2,
                    area.width,
                    area.height / 2,
                ),
            )
        };
        f.render_stateful_widget(w, top, &mut filter_menu_state.column_selection.clone());
        f.render_widget(
            Paragraph::new(filter_menu_state.input.clone()).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Filter pattern"),
            ),
            bottom,
        );
        f.set_cursor_position(Position::new(
            bottom.x + 1 + filter_menu_state.input.len() as u16,
            bottom.y + 1,
        ));
    }

    /// Renders a box showing instructions on how to exit from `vector top`.
    fn quit_box(&self, f: &mut Frame, area: Rect) {
        let text = vec![Line::from(
            "To quit, press ESC or 'q'; Press F1 or '?' for help",
        )];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Gray));
        let w = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Left);

        f.render_widget(w, area);
    }

    /// Draw a single frame. Creates a layout and renders widgets into it.
    fn draw(&self, f: &mut Frame, state: state::State) {
        let size = f.area();
        let rects = Layout::default()
            .constraints(self.constraints.clone())
            .split(size);

        self.title(f, rects[0], &state.connection_status, state.uptime);

        // Require a minimum of 80 chars of line width to display the table
        if size.width >= 80 {
            self.components_table(f, &state, rects[1]);
        } else {
            self.components_resize_window(f, rects[1]);
        }

        self.quit_box(f, rects[2]);

        // Render help, sort and filter over other items
        if state.ui.help_visible {
            let [area] = Layout::horizontal([Constraint::Length(64)])
                .flex(Flex::Center)
                .areas(size);
            let [area] = Layout::vertical([Constraint::Length(32)])
                .flex(Flex::Center)
                .areas(area);
            self.help_box(f, area);
        }

        if state.ui.sort_visible {
            let [area] = Layout::horizontal([Constraint::Length(64)])
                .flex(Flex::Center)
                .areas(size);
            let [area] = Layout::vertical([Constraint::Length(32)])
                .flex(Flex::Center)
                .areas(area);
            self.sort_box(f, area, state.ui.sort_menu_state);
        }

        if state.ui.filter_visible {
            let [area] = Layout::horizontal([Constraint::Length(64)])
                .flex(Flex::Center)
                .areas(size);
            let [area] = Layout::vertical([Constraint::Length(12)])
                .flex(Flex::Center)
                .areas(area);
            self.filter_box(f, area, &state.ui.filter_menu_state);
        }
    }
}

/// Determine if the terminal is a TTY
pub fn is_tty() -> bool {
    stdout().is_tty()
}

/// Initialize the dashboard. A new terminal drawing session will be created, targeting
/// stdout. We're using 'direct' drawing mode to control the full output of the dashboard,
/// as well as entering an 'alternate screen' to overlay the console. This ensures that when
/// the dashboard is exited, the user's previous terminal session can commence, unaffected.
pub async fn init_dashboard<'a>(
    title: &'a str,
    url: &'a str,
    interval: u32,
    human_metrics: bool,
    event_tx: state::EventTx,
    mut state_rx: state::StateRx,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Capture key presses, to determine when to quit
    let (mut key_press_rx, key_press_kill_tx) = capture_key_press();

    // Write to stdout, and enter an alternate screen, to avoid overwriting existing
    // terminal output
    let mut stdout = stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Drop into 'raw' mode, to enable direct drawing to the terminal
    enable_raw_mode()?;

    // Build terminal. We're using crossterm for *nix + Windows support
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Clear the screen, readying it for output
    terminal.clear()?;

    let widgets = Widgets::new(title, url, interval, human_metrics);
    let mut input_mode = InputMode::Top;

    loop {
        tokio::select! {
            Some(state) = state_rx.recv() => {
                if state.ui.filter_visible {
                    input_mode = InputMode::FilterInput;
                } else if state.ui.sort_visible {
                    input_mode = InputMode::SortMenu;
                } else if state.ui.help_visible {
                    input_mode = InputMode::HelpMenu;
                } else {
                    input_mode = InputMode::Top;
                }
                terminal.draw(|f| widgets.draw(f, state))?;
            },
            k = key_press_rx.recv() => {
                let k = k.unwrap();
                if handle_input(input_mode, k, &event_tx, &terminal).await {
                    _ = key_press_kill_tx.send(());
                    break;
                }
            }
            _ = &mut shutdown_rx => {
                _ = key_press_kill_tx.send(());
                break
            }
        }
    }

    // Clean-up terminal
    terminal.backend_mut().execute(DisableMouseCapture)?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.backend_mut().execute(Show)?;

    disable_raw_mode()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Zero should be formatted as "--" in all cases
    fn format_zero() {
        const N: i64 = 0;

        assert_eq!(N.thousands_format(), "--");
        assert_eq!(N.human_format(), "--");
    }

    #[test]
    /// < 1000 should always be as-is
    fn format_hundred() {
        const N: i64 = 100;

        assert_eq!(N.thousands_format(), "100");
        assert_eq!(N.human_format(), "100");
    }

    #[test]
    /// 1,000+ starts to make a difference...
    fn format_thousands() {
        const N: i64 = 1_000;

        assert_eq!(N.thousands_format(), "1,000");
        assert_eq!(N.human_format(), "1.00 k");
    }

    #[test]
    /// Shouldn't round down
    fn format_thousands_no_rounding() {
        const N: i64 = 1_500;

        assert_eq!(N.thousands_format(), "1,500");
        assert_eq!(N.human_format(), "1.50 k");
    }

    #[test]
    /// Should round down when human formatted
    fn format_thousands_round_down() {
        const N: i64 = 1_514;

        assert_eq!(N.thousands_format(), "1,514");
        assert_eq!(N.human_format(), "1.51 k");
    }

    #[test]
    /// Should round up when human formatted
    fn format_thousands_round_up() {
        const N: i64 = 1_999;

        assert_eq!(N.thousands_format(), "1,999");
        assert_eq!(N.human_format(), "2.00 k");
    }

    #[test]
    /// Should format millions
    fn format_millions() {
        const N: i64 = 1_000_000;

        assert_eq!(N.thousands_format(), "1,000,000");
        assert_eq!(N.human_format(), "1.00 M");
    }

    #[test]
    /// Should format billions
    fn format_billions() {
        const N: i64 = 1_000_000_000;

        assert_eq!(N.thousands_format(), "1,000,000,000");
        assert_eq!(N.human_format(), "1.00 G");
    }

    #[test]
    /// Should format trillions
    fn format_trillions() {
        const N: i64 = 1_100_000_000_000;

        assert_eq!(N.thousands_format(), "1,100,000,000,000");
        assert_eq!(N.human_format(), "1.10 T");
    }

    #[test]
    /// Should format bytes
    fn format_bytes() {
        const N: i64 = 1024;

        assert_eq!(N.human_format_bytes(), "1.00 KiB");
        assert_eq!((N * N).human_format_bytes(), "1.00 MiB");
        assert_eq!((N * (N * N)).human_format_bytes(), "1.00 GiB");
        assert_eq!((N * (N * (N * N))).human_format_bytes(), "1.00 TiB");
        assert_eq!((N * (N * (N * (N * N)))).human_format_bytes(), "1.00 PiB");
    }
}
