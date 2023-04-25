use crate::internal_telemetry::is_allocation_tracking_enabled;
use crossterm::{
    cursor::Show,
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    tty::IsTty,
    ExecutableCommand,
};
use num_format::{Locale, ToFormattedString};
use number_prefix::NumberPrefix;
use std::io::stdout;
use tokio::sync::oneshot;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
    Frame, Terminal,
};

use super::{
    events::capture_key_press,
    state::{self, ConnectionStatus},
};

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
                NumberPrefix::Prefixed(p, n) => format!("{:.2} {}", n, p),
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
                NumberPrefix::Prefixed(p, n) => format!("{:.2} {}B", n, p),
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

const NUM_COLUMNS: usize = if is_allocation_tracking_enabled() {
    9
} else {
    8
};

static HEADER: [&str; NUM_COLUMNS] = [
    "ID",
    "Output",
    "Kind",
    "Type",
    "Events In",
    "Events Out",
    "Bytes",
    "Errors",
    #[cfg(feature = "allocation-tracing")]
    "Mem Usage Bytes",
];

struct Widgets<'a> {
    constraints: Vec<Constraint>,
    url_string: &'a str,
    opts: &'a super::Opts,
}

impl<'a> Widgets<'a> {
    /// Creates a new Widgets, containing constraints to re-use across renders.
    pub fn new(url_string: &'a str, opts: &'a super::Opts) -> Self {
        let constraints = vec![
            Constraint::Length(3),
            Constraint::Max(90),
            Constraint::Length(3),
        ];

        Self {
            constraints,
            url_string,
            opts,
        }
    }

    /// Renders a title showing 'Vector', and the URL the dashboard is currently connected to.
    fn title<B: Backend>(
        &'a self,
        f: &mut Frame<B>,
        area: Rect,
        connection_status: &ConnectionStatus,
    ) {
        let text = vec![Spans::from(vec![
            Span::from(self.url_string),
            Span::styled(
                format!(" | Sampling @ {}ms", self.opts.interval.thousands_format()),
                Style::default().fg(Color::Gray),
            ),
            Span::from(" | "),
            Span::styled(connection_status.to_string(), connection_status.style()),
        ])];

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Vector",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
        let w = Paragraph::new(text).block(block).wrap(Wrap { trim: true });

        f.render_widget(w, area);
    }

    /// Renders a components table, showing sources, transforms and sinks in tabular form, with
    /// statistics pulled from `ComponentsState`,
    fn components_table<B: Backend>(&self, f: &mut Frame<B>, state: &state::State, area: Rect) {
        // Header columns
        let header = HEADER
            .iter()
            .map(|s| Cell::from(*s).style(Style::default().add_modifier(Modifier::BOLD)))
            .collect::<Vec<_>>();

        // Data columns
        let mut items = Vec::new();
        for (_, r) in state.components.iter() {
            let mut data = vec![
                r.key.id().to_string(),
                (!r.has_displayable_outputs())
                    .then_some("--")
                    .unwrap_or_default()
                    .to_string(),
                r.kind.clone(),
                r.component_type.clone(),
            ];

            let formatted_metrics = [
                format_metric(
                    r.received_events_total,
                    r.received_events_throughput_sec,
                    self.opts.human_metrics,
                ),
                format_metric(
                    r.sent_events_total,
                    r.sent_events_throughput_sec,
                    self.opts.human_metrics,
                ),
                format_metric(
                    r.processed_bytes_total,
                    r.processed_bytes_throughput_sec,
                    self.opts.human_metrics,
                ),
                if self.opts.human_metrics {
                    r.errors.human_format()
                } else {
                    r.errors.thousands_format()
                },
                #[cfg(feature = "allocation-tracing")]
                r.allocated_bytes.human_format(),
            ];

            data.extend_from_slice(&formatted_metrics);
            items.push(Row::new(data).style(Style::default()));

            // Add output rows
            if r.has_displayable_outputs() {
                for (id, output) in r.outputs.iter() {
                    let sent_events_metric = format_metric(
                        output.sent_events_total,
                        output.sent_events_throughput_sec,
                        self.opts.human_metrics,
                    );
                    let mut data = [""; NUM_COLUMNS]
                        .into_iter()
                        .map(Cell::from)
                        .collect::<Vec<_>>();
                    data[1] = Cell::from(id.as_ref());
                    data[5] = Cell::from(sent_events_metric);
                    items.push(Row::new(data).style(Style::default()));
                }
            }
        }

        let w = Table::new(items)
            .header(Row::new(header).bottom_margin(1))
            .block(Block::default().borders(Borders::ALL).title("Components"))
            .column_spacing(2)
            .widths(if is_allocation_tracking_enabled() {
                &[
                    Constraint::Percentage(15), // ID
                    Constraint::Percentage(6),  // Output
                    Constraint::Percentage(8),  // Kind
                    Constraint::Percentage(10), // Type
                    Constraint::Percentage(10), // Events In
                    Constraint::Percentage(10), // Events Out
                    Constraint::Percentage(10), // Bytes
                    Constraint::Percentage(5),  // Errors
                    Constraint::Percentage(16), // Allocated Bytes
                ]
            } else {
                &[
                    Constraint::Percentage(15), // ID
                    Constraint::Percentage(15), // Output
                    Constraint::Percentage(10), // Kind
                    Constraint::Percentage(10), // Type
                    Constraint::Percentage(10), // Events In
                    Constraint::Percentage(10), // Events Out
                    Constraint::Percentage(10), // Bytes
                    Constraint::Percentage(10), // Errors
                ]
            });
        f.render_widget(w, area);
    }

    /// Alerts the user to resize the window to view columns
    fn components_resize_window<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Components");
        let w = Paragraph::new("Expand the window to > 80 chars to view metrics")
            .block(block)
            .wrap(Wrap { trim: true });

        f.render_widget(w, area);
    }

    /// Renders a box showing instructions on how to exit from `vector top`.
    fn quit_box<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        let text = vec![Spans::from("To quit, press ESC or 'q'")];

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
    fn draw<B: Backend>(&self, f: &mut Frame<B>, state: state::State) {
        let size = f.size();
        let rects = Layout::default()
            .constraints(self.constraints.clone())
            .split(size);

        self.title(f, rects[0], &state.connection_status);

        // Require a minimum of 80 chars of line width to display the table
        if size.width >= 80 {
            self.components_table(f, &state, rects[1]);
        } else {
            self.components_resize_window(f, rects[1]);
        }

        self.quit_box(f, rects[2]);
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
    url: &'a str,
    opts: &'a super::Opts,
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

    let widgets = Widgets::new(url, opts);

    loop {
        tokio::select! {
            Some(state) = state_rx.recv() => {
                terminal.draw(|f| widgets.draw(f, state))?;
            },
            k = key_press_rx.recv() => {
                if let KeyCode::Esc | KeyCode::Char('q') = k.unwrap() {
                    _ = key_press_kill_tx.send(());
                    break
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
