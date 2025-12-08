use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use chrono::{DateTime, Local};
use ratatui::{
    layout::Size,
    style::{Color, Style},
    text::Span,
    widgets::ListState,
};
use tokio::sync::mpsc;
use vector_common::internal_event::DEFAULT_OUTPUT;

use vector_common::config::ComponentKey;

type IdentifiedMetric = (ComponentKey, i64);

#[derive(Debug)]
pub struct SentEventsMetric {
    pub key: ComponentKey,
    pub total: i64,
    pub outputs: HashMap<String, i64>,
}

#[derive(Debug)]
pub enum EventType {
    InitializeState(State),
    UptimeChanged(f64),
    ReceivedBytesTotals(Vec<IdentifiedMetric>),
    /// Interval + identified metric
    ReceivedBytesThroughputs(i64, Vec<IdentifiedMetric>),
    ReceivedEventsTotals(Vec<IdentifiedMetric>),
    /// Interval in ms + identified metric
    ReceivedEventsThroughputs(i64, Vec<IdentifiedMetric>),
    SentBytesTotals(Vec<IdentifiedMetric>),
    /// Interval + identified metric
    SentBytesThroughputs(i64, Vec<IdentifiedMetric>),
    // Identified overall metric + output-specific metrics
    SentEventsTotals(Vec<SentEventsMetric>),
    /// Interval in ms + identified overall metric + output-specific metrics
    SentEventsThroughputs(i64, Vec<SentEventsMetric>),
    ErrorsTotals(Vec<IdentifiedMetric>),
    #[cfg(feature = "allocation-tracing")]
    AllocatedBytes(Vec<IdentifiedMetric>),
    ComponentAdded(ComponentRow),
    ComponentRemoved(ComponentKey),
    ConnectionUpdated(ConnectionStatus),
    Ui(UiEventType),
}

#[derive(Debug)]
pub enum UiEventType {
    // Scroll up (-) or down (+). Also passes the window size for correct max scroll calculation.
    Scroll(isize, Size),
    // Scroll up (-) or down (+) by a whole page. Also passes the window size for page size and max scroll calculation.
    ScrollPage(isize, Size),
    // Toggles help window. Also closes other windows.
    ToggleHelp,
    // Toggles sort menu. Also closes other windows.
    ToggleSortMenu,
    // Toggles sort direction.
    ToggleSortDirection,
    // Change sort selection up (-) or down (+).
    SortSelection(isize),
    // Change sort selection to a specific column.
    SortByColumn(SortColumn),
    // Confirms current sort selection.
    SortConfirmation,
}

#[derive(Debug, Copy, Clone)]
pub enum ConnectionStatus {
    // Initial state
    Pending,
    // Underlying web socket connection has dropped. Includes the delay between
    // reconnect attempts
    Disconnected(u64),
    // Connection is working
    Connected(DateTime<Local>),
}

impl ConnectionStatus {
    pub fn as_ui_spans(&self) -> Vec<Span<'_>> {
        match self {
            Self::Pending => vec![Span::styled(
                "Connecting...",
                Style::default().fg(Color::Yellow),
            )],
            Self::Disconnected(delay) => vec![
                Span::styled("Disconnected", Style::default().fg(Color::Red)),
                Span::from(format!(" (reconnecting every {} seconds)", delay / 1000)),
            ],
            Self::Connected(since) => vec![
                Span::styled("Connected", Style::default().fg(Color::Green)),
                Span::from(format!(" (since {})", since.format("%F %r %Z"))),
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub connection_status: ConnectionStatus,
    pub uptime: Duration,
    pub components: BTreeMap<ComponentKey, ComponentRow>,
    pub sort_state: SortState,
    pub ui: UiState,
}

#[derive(Debug, Clone, Copy)]
pub enum SortColumn {
    Id = 0,
    Kind = 1,
    Type = 2,
    EventsIn = 3,
    EventsInTotal = 4,
    BytesIn = 5,
    BytesInTotal = 6,
    EventsOut = 7,
    EventsOutTotal = 8,
    BytesOut = 9,
    BytesOutTotal = 10,
    Errors = 11,
    #[cfg(feature = "allocation-tracing")]
    MemoryUsed = 12,
}

impl SortColumn {
    pub fn matches_header(&self, header: &str) -> bool {
        match self {
            SortColumn::Id => header == "ID",
            SortColumn::Kind => header == "Kind",
            SortColumn::Type => header == "Type",
            SortColumn::EventsIn | SortColumn::EventsInTotal => header == "Events In",
            SortColumn::BytesIn | SortColumn::BytesInTotal => header == "Bytes In",
            SortColumn::EventsOut | SortColumn::EventsOutTotal => header == "Events Out",
            SortColumn::BytesOut | SortColumn::BytesOutTotal => header == "Bytes Out",
            SortColumn::Errors => header == "Errors",
            #[cfg(feature = "allocation-tracing")]
            SortColumn::MemoryUsed => header == "Memory Used",
        }
    }

    pub fn items() -> Vec<&'static str> {
        vec![
            "ID",
            "Kind",
            "Type",
            "Events In",
            "Events In Total",
            "Bytes In",
            "Bytes In Total",
            "Events Out",
            "Events Out Total",
            "Bytes Out",
            "Bytes Out Total",
            "Errors",
            #[cfg(feature = "allocation-tracing")]
            "Memory Used",
        ]
    }
}

impl From<usize> for SortColumn {
    fn from(value: usize) -> Self {
        match value {
            1 => SortColumn::Kind,
            2 => SortColumn::Type,
            3 => SortColumn::EventsIn,
            4 => SortColumn::EventsInTotal,
            5 => SortColumn::BytesIn,
            6 => SortColumn::BytesInTotal,
            7 => SortColumn::EventsOut,
            8 => SortColumn::EventsOutTotal,
            9 => SortColumn::BytesOut,
            10 => SortColumn::BytesOutTotal,
            11 => SortColumn::Errors,
            #[cfg(feature = "allocation-tracing")]
            12 => SortColumn::MemoryUsed,
            _ => SortColumn::Id,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SortState {
    pub column: Option<SortColumn>,
    pub reverse: bool,
}

#[derive(Debug, Default, Clone)]
pub struct UiState {
    pub scroll: usize,
    pub help_visible: bool,
    pub sort_visible: bool,
    pub sort_menu_state: ListState,
}

impl UiState {
    /// Returns the height of components display box in rows, based on provided [`Size`].
    /// Calculates by deducting rows used for header and footer.
    pub fn components_box_height(area: Size) -> u16 {
        // Currently hardcoded (10 is the number of rows the header and footer take up)
        area.height.saturating_sub(10)
    }

    /// Returns the maximum scroll value
    pub fn max_scroll(area: Size, components_count: usize) -> usize {
        components_count.saturating_sub(Self::components_box_height(area).into())
    }

    /// Changes current scroll by provided diff in rows. Uses [`Size`] to limit scroll,
    /// so that scrolling down is possible until the last component is visible.
    pub fn scroll(&mut self, diff: isize, area: Size, components_count: usize) {
        let max_scroll = Self::max_scroll(area, components_count);
        self.scroll = self.scroll.saturating_add_signed(diff);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// Changes current scroll by provided diff in pages. Uses [`Size`] to limit scroll,
    /// and to calculate number of rows a page contains.
    pub fn scroll_page(&mut self, diff: isize, area: Size, components_count: usize) {
        self.scroll(
            diff * (Self::components_box_height(area) as isize),
            area,
            components_count,
        );
    }
}

impl State {
    pub fn new(components: BTreeMap<ComponentKey, ComponentRow>) -> Self {
        Self {
            connection_status: ConnectionStatus::Pending,
            uptime: Duration::from_secs(0),
            components,
            ui: UiState::default(),
            sort_state: SortState::default(),
        }
    }
}
pub type EventTx = mpsc::Sender<EventType>;
pub type EventRx = mpsc::Receiver<EventType>;
pub type StateRx = mpsc::Receiver<State>;

#[derive(Debug, Clone, Default)]
pub struct OutputMetrics {
    pub sent_events_total: i64,
    pub sent_events_throughput_sec: i64,
}

impl From<i64> for OutputMetrics {
    fn from(sent_events_total: i64) -> Self {
        Self {
            sent_events_total,
            sent_events_throughput_sec: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComponentRow {
    pub key: ComponentKey,
    pub kind: String,
    pub component_type: String,
    pub outputs: HashMap<String, OutputMetrics>,
    pub received_bytes_total: i64,
    pub received_bytes_throughput_sec: i64,
    pub received_events_total: i64,
    pub received_events_throughput_sec: i64,
    pub sent_bytes_total: i64,
    pub sent_bytes_throughput_sec: i64,
    pub sent_events_total: i64,
    pub sent_events_throughput_sec: i64,
    #[cfg(feature = "allocation-tracing")]
    pub allocated_bytes: i64,
    pub errors: i64,
}

impl ComponentRow {
    /// Note, we ignore `outputs` if it only contains [`DEFAULT_OUTPUT`] to avoid
    /// redundancy with information shown in the overall component row
    pub fn has_displayable_outputs(&self) -> bool {
        self.outputs.len() > 1
            || (self.outputs.len() == 1 && !self.outputs.contains_key(DEFAULT_OUTPUT))
    }
}

/// Takes the receiver `EventRx` channel, and returns a `StateRx` state receiver. This
/// represents the single destination for handling subscriptions and returning 'immutable' state
/// for re-rendering the dashboard. This approach uses channels vs. mutexes.
pub async fn updater(mut event_rx: EventRx) -> StateRx {
    let (tx, rx) = mpsc::channel(20);

    let mut state = State::new(BTreeMap::new());
    tokio::spawn(async move {
        while let Some(event_type) = event_rx.recv().await {
            match event_type {
                EventType::InitializeState(new_state) => {
                    state = new_state;
                }
                EventType::ReceivedBytesTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.received_bytes_total = v;
                        }
                    }
                }
                EventType::ReceivedBytesThroughputs(interval, rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.received_bytes_throughput_sec =
                                (v as f64 * (1000.0 / interval as f64)) as i64;
                        }
                    }
                }
                EventType::ReceivedEventsTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.received_events_total = v;
                        }
                    }
                }
                EventType::ReceivedEventsThroughputs(interval, rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.received_events_throughput_sec =
                                (v as f64 * (1000.0 / interval as f64)) as i64;
                        }
                    }
                }
                EventType::SentBytesTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.sent_bytes_total = v;
                        }
                    }
                }
                EventType::SentBytesThroughputs(interval, rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.sent_bytes_throughput_sec =
                                (v as f64 * (1000.0 / interval as f64)) as i64;
                        }
                    }
                }
                EventType::SentEventsTotals(rows) => {
                    for m in rows {
                        if let Some(r) = state.components.get_mut(&m.key) {
                            r.sent_events_total = m.total;
                            for (id, v) in m.outputs {
                                r.outputs
                                    .entry(id)
                                    .or_insert_with(OutputMetrics::default)
                                    .sent_events_total = v;
                            }
                        }
                    }
                }
                EventType::SentEventsThroughputs(interval, rows) => {
                    for m in rows {
                        if let Some(r) = state.components.get_mut(&m.key) {
                            r.sent_events_throughput_sec =
                                (m.total as f64 * (1000.0 / interval as f64)) as i64;
                            for (id, v) in m.outputs {
                                let throughput = (v as f64 * (1000.0 / interval as f64)) as i64;
                                r.outputs
                                    .entry(id)
                                    .or_insert_with(OutputMetrics::default)
                                    .sent_events_throughput_sec = throughput;
                            }
                        }
                    }
                }
                EventType::ErrorsTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.errors = v;
                        }
                    }
                }
                #[cfg(feature = "allocation-tracing")]
                EventType::AllocatedBytes(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.allocated_bytes = v;
                        }
                    }
                }
                EventType::ComponentAdded(c) => {
                    _ = state.components.insert(c.key.clone(), c);
                }
                EventType::ComponentRemoved(key) => {
                    _ = state.components.remove(&key);
                }
                EventType::ConnectionUpdated(status) => {
                    state.connection_status = status;
                }
                EventType::UptimeChanged(uptime) => {
                    state.uptime = Duration::from_secs_f64(uptime);
                }
                EventType::Ui(ui_event_type) => match ui_event_type {
                    UiEventType::Scroll(diff, area) => {
                        state.ui.scroll(diff, area, state.components.len());
                    }
                    UiEventType::ScrollPage(diff, area) => {
                        state.ui.scroll_page(diff, area, state.components.len());
                    }
                    UiEventType::ToggleHelp => {
                        state.ui.help_visible = !state.ui.help_visible;
                        if state.ui.help_visible {
                            state.ui.sort_visible = false;
                        }
                    }
                    UiEventType::ToggleSortMenu => {
                        state.ui.sort_visible = !state.ui.sort_visible;
                        state
                            .ui
                            .sort_menu_state
                            .select(state.sort_state.column.map(|c| c as usize));
                        if state.ui.sort_visible {
                            state.ui.help_visible = false;
                        }
                    }
                    UiEventType::ToggleSortDirection => {
                        state.sort_state.reverse = !state.sort_state.reverse
                    }
                    UiEventType::SortSelection(diff) => {
                        let next = state.ui.sort_menu_state.selected().map_or(0, |s| {
                            s.saturating_add_signed(diff)
                                .min(SortColumn::items().len() - 1)
                        });
                        state.ui.sort_menu_state.select(Some(next));
                    }
                    UiEventType::SortByColumn(col) => state.sort_state.column = Some(col),
                    UiEventType::SortConfirmation => {
                        if let Some(selected) = state.ui.sort_menu_state.selected() {
                            state.sort_state.column = Some(selected.into())
                        }
                        state.ui.sort_visible = false;
                    }
                },
            }

            // Send updated map to listeners
            _ = tx.send(state.clone()).await;
        }
    });

    rx
}
