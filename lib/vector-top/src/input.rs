use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, prelude::Backend};

use crate::state::{self, EventType, SortColumn, UiEventType};

#[derive(Debug, Clone, Copy)]
pub(crate) enum InputMode {
    Top,
    HelpMenu,
    FilterInput,
    SortMenu,
}

/// Handles keyboard input for top
///
/// Returns true if input handling is done (quit is requested)
pub(crate) async fn handle_input<B: Backend>(
    mode: InputMode,
    key_event: KeyEvent,
    event_tx: &state::EventTx,
    terminal: &Terminal<B>,
) -> bool {
    match mode {
        InputMode::Top => handle_top_input(key_event, event_tx, terminal).await,
        InputMode::HelpMenu => handle_help_input(key_event, event_tx, terminal).await,
        InputMode::FilterInput => todo!(),
        InputMode::SortMenu => handle_sort_input(key_event, event_tx, terminal).await,
    }
}

async fn handle_top_input<B: Backend>(
    key_event: KeyEvent,
    event_tx: &state::EventTx,
    terminal: &Terminal<B>,
) -> bool {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            return true;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::Scroll(
                    -1,
                    terminal.size().unwrap_or_default(),
                )))
                .await;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::Scroll(
                    1,
                    terminal.size().unwrap_or_default(),
                )))
                .await;
        }
        KeyCode::End | KeyCode::Char('G') => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::Scroll(
                    isize::MAX,
                    terminal.size().unwrap_or_default(),
                )))
                .await;
        }
        KeyCode::Home | KeyCode::Char('g') => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::Scroll(
                    isize::MIN,
                    terminal.size().unwrap_or_default(),
                )))
                .await;
        }
        KeyCode::Left | KeyCode::PageUp => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::ScrollPage(
                    -1,
                    terminal.size().unwrap_or_default(),
                )))
                .await;
        }
        KeyCode::Char('b') if key_event.modifiers.intersects(KeyModifiers::CONTROL) => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::ScrollPage(
                    -1,
                    terminal.size().unwrap_or_default(),
                )))
                .await;
        }
        KeyCode::Right | KeyCode::PageDown => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::ScrollPage(
                    1,
                    terminal.size().unwrap_or_default(),
                )))
                .await;
        }
        KeyCode::Char('f') if key_event.modifiers.intersects(KeyModifiers::CONTROL) => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::ScrollPage(
                    1,
                    terminal.size().unwrap_or_default(),
                )))
                .await;
        }
        KeyCode::Char('?') | KeyCode::F(1) => {
            let _ = event_tx.send(EventType::Ui(UiEventType::ToggleHelp)).await;
        }
        KeyCode::Char('s') | KeyCode::F(6) => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::ToggleSortMenu))
                .await;
        }
        KeyCode::Char('r') | KeyCode::F(7) => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::ToggleSortDirection))
                .await;
        }
        KeyCode::Char(d) if d.is_ascii_digit() => {
            let col = match d {
                '1' => SortColumn::Id,
                '3' => SortColumn::Kind,
                '4' => SortColumn::Type,
                '5' => SortColumn::EventsInTotal,
                '6' => SortColumn::BytesInTotal,
                '7' => SortColumn::EventsOutTotal,
                '8' => SortColumn::BytesOutTotal,
                '9' => SortColumn::Errors,
                #[cfg(feature = "allocation-tracing")]
                '0' => SortColumn::MemoryUsed,
                _ => return false,
            };
            let _ = event_tx
                .send(EventType::Ui(UiEventType::SortByColumn(col)))
                .await;
        }
        _ => (),
    }
    false
}

async fn handle_help_input<B: Backend>(
    key_event: KeyEvent,
    event_tx: &state::EventTx,
    terminal: &Terminal<B>,
) -> bool {
    match key_event.code {
        KeyCode::Esc => {
            let _ = event_tx.send(EventType::Ui(UiEventType::ToggleHelp)).await;
        }
        _ => return handle_top_input(key_event, event_tx, terminal).await,
    }
    false
}

async fn handle_sort_input<B: Backend>(
    key_event: KeyEvent,
    event_tx: &state::EventTx,
    terminal: &Terminal<B>,
) -> bool {
    match key_event.code {
        KeyCode::Esc => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::ToggleSortMenu))
                .await;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::SortSelection(-1)))
                .await;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::SortSelection(1)))
                .await;
        }
        KeyCode::Enter => {
            let _ = event_tx
                .send(EventType::Ui(UiEventType::SortConfirmation))
                .await;
        }
        _ => return handle_top_input(key_event, event_tx, terminal).await,
    }
    false
}
