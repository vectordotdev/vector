use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, prelude::Backend};

use crate::state::{self, SortColumn, UiEventType};

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
    event_tx: &state::UiEventTx,
    terminal: &Terminal<B>,
) -> bool {
    match mode {
        InputMode::Top => handle_top_input(key_event, event_tx, terminal).await,
        InputMode::HelpMenu => handle_help_input(key_event, event_tx, terminal).await,
        InputMode::FilterInput => handle_filter_input(key_event, event_tx, terminal).await,
        InputMode::SortMenu => handle_sort_input(key_event, event_tx, terminal).await,
    }
}

async fn handle_top_input<B: Backend>(
    key_event: KeyEvent,
    event_tx: &state::UiEventTx,
    terminal: &Terminal<B>,
) -> bool {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            return true;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            event_tx
                .send(UiEventType::Scroll(-1, terminal.size().unwrap_or_default()))
                .await
                .ok();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            event_tx
                .send(UiEventType::Scroll(1, terminal.size().unwrap_or_default()))
                .await
                .ok();
        }
        KeyCode::End | KeyCode::Char('G') => {
            event_tx
                .send(UiEventType::Scroll(
                    isize::MAX,
                    terminal.size().unwrap_or_default(),
                ))
                .await
                .ok();
        }
        KeyCode::Home | KeyCode::Char('g') => {
            event_tx
                .send(UiEventType::Scroll(
                    isize::MIN,
                    terminal.size().unwrap_or_default(),
                ))
                .await
                .ok();
        }
        KeyCode::Left | KeyCode::PageUp => {
            event_tx
                .send(UiEventType::ScrollPage(
                    -1,
                    terminal.size().unwrap_or_default(),
                ))
                .await
                .ok();
        }
        KeyCode::Char('b') if key_event.modifiers.intersects(KeyModifiers::CONTROL) => {
            event_tx
                .send(UiEventType::ScrollPage(
                    -1,
                    terminal.size().unwrap_or_default(),
                ))
                .await
                .ok();
        }
        KeyCode::Right | KeyCode::PageDown => {
            event_tx
                .send(UiEventType::ScrollPage(
                    1,
                    terminal.size().unwrap_or_default(),
                ))
                .await
                .ok();
        }
        KeyCode::Char('f') if key_event.modifiers.intersects(KeyModifiers::CONTROL) => {
            event_tx
                .send(UiEventType::ScrollPage(
                    1,
                    terminal.size().unwrap_or_default(),
                ))
                .await
                .ok();
        }
        KeyCode::Char('?') | KeyCode::F(1) => {
            event_tx.send(UiEventType::ToggleHelp).await.ok();
        }
        KeyCode::Char('s') | KeyCode::F(6) => {
            event_tx.send(UiEventType::ToggleSortMenu).await.ok();
        }
        KeyCode::Char('r') | KeyCode::F(7) => {
            event_tx.send(UiEventType::ToggleSortDirection).await.ok();
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
            event_tx.send(UiEventType::SortByColumn(col)).await.ok();
        }
        KeyCode::F(4) | KeyCode::Char('f') | KeyCode::Char('/') => {
            event_tx.send(UiEventType::ToggleFilterMenu).await.ok();
        }
        _ => (),
    }
    false
}

async fn handle_help_input<B: Backend>(
    key_event: KeyEvent,
    event_tx: &state::UiEventTx,
    terminal: &Terminal<B>,
) -> bool {
    match key_event.code {
        KeyCode::Esc => {
            event_tx.send(UiEventType::ToggleHelp).await.ok();
        }
        _ => return handle_top_input(key_event, event_tx, terminal).await,
    }
    false
}

async fn handle_sort_input<B: Backend>(
    key_event: KeyEvent,
    event_tx: &state::UiEventTx,
    terminal: &Terminal<B>,
) -> bool {
    match key_event.code {
        KeyCode::Esc => {
            event_tx.send(UiEventType::ToggleSortMenu).await.ok();
        }
        KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => {
            event_tx.send(UiEventType::SortSelection(-1)).await.ok();
        }
        KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => {
            event_tx.send(UiEventType::SortSelection(1)).await.ok();
        }
        KeyCode::Enter => {
            event_tx.send(UiEventType::SortConfirmation).await.ok();
        }
        _ => return handle_top_input(key_event, event_tx, terminal).await,
    }
    false
}

async fn handle_filter_input<B: Backend>(
    key_event: KeyEvent,
    event_tx: &state::UiEventTx,
    terminal: &Terminal<B>,
) -> bool {
    match key_event.code {
        KeyCode::Esc => {
            event_tx.send(UiEventType::ToggleFilterMenu).await.ok();
        }
        KeyCode::BackTab | KeyCode::Up => {
            event_tx
                .send(UiEventType::FilterColumnSelection(-1))
                .await
                .ok();
        }
        KeyCode::Tab | KeyCode::Down => {
            event_tx
                .send(UiEventType::FilterColumnSelection(1))
                .await
                .ok();
        }
        KeyCode::Backspace => {
            event_tx.send(UiEventType::FilterBackspace).await.ok();
        }
        KeyCode::Enter => {
            event_tx.send(UiEventType::FilterConfirmation).await.ok();
        }
        KeyCode::Char(any) => {
            event_tx.send(UiEventType::FilterInput(any)).await.ok();
        }
        _ => return handle_top_input(key_event, event_tx, terminal).await,
    }
    false
}
