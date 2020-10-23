use super::{
    events::capture_key_press,
    state::{WidgetsState, TOPOLOGY_HEADERS},
};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use std::{
    io::{stdout, Write},
    sync::Arc,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame, Terminal,
};

pub struct Widgets {
    constraints: Vec<Constraint>,
    state: Arc<WidgetsState>,
}

impl Widgets {
    pub fn new(state: Arc<WidgetsState>) -> Self {
        let constraints = vec![
            Constraint::Length(3),
            Constraint::Max(90),
            Constraint::Length(3),
        ];

        Self { constraints, state }
    }

    /// Renders a title showing 'Vector', and the URL the dashboard is currently connected to
    fn title<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        let text = vec![Spans::from(self.state.url())];

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Vector",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
        let w = Paragraph::new(text).block(block).wrap(Wrap { trim: true });

        f.render_widget(w, area);
    }

    /// Renders a topology table, showing sources, transforms and sinks in tabular form, with
    /// statistics pulled from `topology_state`
    fn topology_table<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        let topology = self.state.topology();
        let items = topology.rows().into_iter().map(|r| {
            Row::StyledData(
                vec![
                    r.name.clone(),
                    r.topology_type.clone(),
                    r.format_events_processed_total(),
                    r.format_errors(),
                    r.format_throughput(),
                ]
                .into_iter(),
                Style::default().fg(Color::White),
            )
        });

        let w = Table::new(TOPOLOGY_HEADERS.iter(), items)
            .block(Block::default().borders(Borders::ALL).title("Topology"))
            .header_gap(1)
            .column_spacing(2)
            .widths(&[
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ]);

        f.render_widget(w, area);
    }

    /// Renders a box showing instructions on how to exit from `vector top`
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

    fn draw<B: Backend>(&self, f: &mut Frame<B>) {
        let rects = Layout::default()
            .constraints(self.constraints.as_ref())
            .split(f.size());

        self.title(f, rects[0]);
        self.topology_table(f, rects[1]);
        self.quit_box(f, rects[2]);
    }

    /// Listen for state updates. Used to determine when to redraw
    async fn listen(&self) {
        self.state.listen().await;
    }
}

/// Initialize the dashboard. A new terminal drawing session will be created, targeting
/// stdout. We're using 'direct' drawing mode to control the full output of the dashboard,
/// as well as entering an 'alternate screen' to overlay the console. This ensures that when
/// the dashboard is exited, the user's previous terminal session can commence, unaffected.
pub async fn init_dashboard(widgets: &Widgets) -> Result<(), Box<dyn std::error::Error>> {
    // Capture key presses, to determine when to quit
    let (mut key_press_rx, key_press_kill) = capture_key_press();

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

    loop {
        tokio::select! {
            _ = widgets.listen() => {
                terminal.draw(|f| widgets.draw(f))?;
            },
            k = key_press_rx.recv() => {
                match k.unwrap() {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        let _ = key_press_kill.send(());
                        break
                    },
                    _ => {},
                };
            }
        }
    }

    // Clean-up terminal
    terminal.backend_mut().execute(DisableMouseCapture)?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;

    disable_raw_mode()?;

    Ok(())
}
