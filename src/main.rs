mod player;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::{error::Error, io};

use clipboard::{ClipboardContext, ClipboardProvider};
use termion::{
    event::Key, input::MouseTerminal, input::TermRead, raw::IntoRawMode, screen::AlternateScreen,
};

use tui::{
    backend::TermionBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Gauge, Paragraph},
    Terminal,
};
use unicode_width::UnicodeWidthStr;

struct App {
    input: String,
    focused: Focused,
}

impl App {
    fn new() -> App {
        App {
            // input: "https://www.franceinter.fr/direct".into(),
            input: "https://www.youtube.com/watch?v=ukzOgoLjHLk".into(),
            focused: Focused::Input,
        }
    }

    fn style_input_chunk(&self) -> Style {
        match self.focused {
            Focused::Input => Style::default().fg(Color::Blue),
            _ => Style::default().fg(Color::White),
        }
    }

    fn style_player_chunk(&self) -> Style {
        match self.focused {
            Focused::Player(_) => Style::default().fg(Color::Blue),
            _ => Style::default().fg(Color::White),
        }
    }

    fn style_play_pause_control(&self) -> Style {
        match self.focused {
            Focused::Player(PlayerButton::PlayPause) => Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(Color::White),
        }
    }

    fn style_stop_control(&self) -> Style {
        match self.focused {
            Focused::Player(PlayerButton::Stop) => Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(Color::White),
        }
    }
}

enum PlayerButton {
    PlayPause,
    Stop,
}

enum Focused {
    Input,
    Player(PlayerButton),
}

enum Event {
    Input(Key),
    Tick,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Terminal initialization
    let stdout = io::stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup shared event channel
    let (tx_events, rx_events) = mpsc::channel();

    // Thread to capture user input
    let tx_input = tx_events.clone();
    thread::spawn(move || {
        let stdin = io::stdin();
        for evt in stdin.keys() {
            if let Ok(key) = evt {
                tx_input.send(Event::Input(key)).ok();
            }
        }
    });

    // Thread to tick for ui refresh
    let tick_rate = Duration::from_millis(250);
    let tx_timer = tx_events.clone();
    thread::spawn(move || loop {
        if tx_timer.send(Event::Tick).is_err() {
            break;
        }
        thread::sleep(tick_rate);
    });

    let mut app = App::new();
    let mut player = player::Player::new()?;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Length(7),
                        Constraint::Min(0),
                    ]
                    .as_ref(),
                )
                .split(f.size());

            let input = Paragraph::new(app.input.as_ref()).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled("Stream", app.style_input_chunk()))
                    .border_style(app.style_input_chunk()),
            );
            f.render_widget(input, chunks[0]);

            if let Focused::Input = app.focused {
                f.set_cursor(chunks[0].x + app.input.width() as u16 + 1, chunks[0].y + 1);
            }

            let pplayer = Block::default()
                .borders(Borders::ALL)
                .title("Player")
                .style(app.style_player_chunk());
            f.render_widget(pplayer, chunks[1]);

            let pplayer = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(2),
                        Constraint::Length(3),
                        Constraint::Min(0),
                    ]
                    .as_ref(),
                )
                .split(chunks[1]);

            let gauge = Gauge::default()
                .block(
                    Block::default()
                        .title(player.title())
                        .style(Style::default().fg(Color::White)),
                )
                .gauge_style(Style::default().fg(Color::LightBlue).bg(Color::DarkGray))
                .ratio(player.progress())
                .label("");
            f.render_widget(gauge, pplayer[0]);

            let controls = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Length(5),
                        Constraint::Length(5),
                        Constraint::Length(5),
                        Constraint::Min(0),
                    ]
                    .as_ref(),
                )
                .split(pplayer[1]);

            let play_pause = if player.playing() { "⏸" } else { "▶" };
            let play_pause = Paragraph::new(play_pause)
                .style(app.style_play_pause_control())
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(play_pause, controls[0]);

            let stop = Paragraph::new("⏹")
                .style(app.style_stop_control())
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(stop, controls[1]);
        })?;

        match (&app.focused, rx_events.recv()?) {
            (_, Event::Input(input)) if Key::Ctrl('c') == input => break,
            (_, Event::Tick) => (),

            (Focused::Input, Event::Input(input)) => match input {
                Key::Char('\t') => app.focused = Focused::Player(PlayerButton::PlayPause),
                Key::BackTab => app.focused = Focused::Player(PlayerButton::Stop),
                Key::Ctrl('h') => app.input.clear(),
                Key::Char('\n') => player.set_uri(&app.input),
                Key::Ctrl('v') => app.input += &get_clipboard_contents(),
                Key::Char(c) => app.input.push(c),
                Key::Backspace => {
                    app.input.pop();
                }
                _ => (),
            },

            (Focused::Player(control), Event::Input(input)) => match input {
                Key::Char('\t') => {
                    app.focused = match control {
                        PlayerButton::PlayPause => Focused::Player(PlayerButton::Stop),
                        PlayerButton::Stop => Focused::Input,
                    }
                }
                Key::BackTab => {
                    app.focused = match control {
                        PlayerButton::PlayPause => Focused::Input,
                        PlayerButton::Stop => Focused::Player(PlayerButton::PlayPause),
                    }
                }
                Key::Char('\n') => {
                    match control {
                        PlayerButton::PlayPause => match player.playing() {
                            true => player.pause(),
                            false => player.play(),
                        },
                        PlayerButton::Stop => player.stop(),
                    };
                }
                _ => (),
            },
        }
    }

    Ok(())
}

pub fn get_clipboard_contents() -> String {
    let clipboard_context: Result<ClipboardContext, Box<dyn Error>> = ClipboardProvider::new();
    match clipboard_context {
        Ok(mut v) => v.get_contents().unwrap_or_default(),
        Err(_) => String::new(),
    }
}
