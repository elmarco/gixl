use color_eyre::Result;
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use gix::{
    bstr::{BString, ByteSlice},
    date::Time,
};
use ratatui::{prelude::*, widgets::*};
use std::{io::stdout, path::PathBuf, process::Command};

#[derive(Clone, Debug)]
pub struct LogEntryInfo {
    pub commit_id: String,
    pub author: BString,
    pub time: String,
    pub message: BString,
    pub author_time: Time,
}

pub type Item<'repo> = (LogEntryInfo, Option<&'repo gix::Submodule<'repo>>);

struct App<'repo> {
    git_dir: PathBuf,
    items: Vec<Item<'repo>>,
    list_items: List<'static>,
    state: ListState,
    list_height: u16,
}

impl<'repo> App<'repo> {
    fn new(git_dir: PathBuf, items: Vec<Item<'repo>>) -> App<'repo> {
        let list_items = build_list_items(&items);
        App {
            git_dir,
            items,
            state: ListState::default(),
            list_height: 0,
            list_items,
        }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i < self.items.len() - 1 {
                    i + 1
                } else {
                    i
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i > 0 {
                    i - 1
                } else {
                    i
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn page_down(&mut self) {
        let page_size = (self.list_height / 2).max(1) as usize;
        let i = match self.state.selected() {
            Some(i) => {
                let next = i + page_size;
                if next >= self.items.len() {
                    self.items.len() - 1
                } else {
                    next
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn page_up(&mut self) {
        let page_size = (self.list_height / 2).max(1) as usize;
        let i = match self.state.selected() {
            Some(i) => i.saturating_sub(page_size),
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn go_to_start(&mut self) {
        self.state.select(Some(0));
    }

    pub fn go_to_end(&mut self) {
        self.state.select(Some(self.items.len() - 1));
    }
}

fn build_list_items<'repo>(items: &[Item<'repo>]) -> List<'static> {
    let mut list_items: Vec<ListItem> = Vec::with_capacity(items.len());
    let mut prev_submodule: Option<&gix::Submodule> = None;
    for i in items {
        let message_lines = i.0.message.split(|c| *c == b'\n').collect::<Vec<_>>();
        let first_line = String::from_utf8_lossy(message_lines[0]).into_owned();
        let author_str = i.0.author.to_str_lossy();
        let author = if author_str.len() > 20 {
            format!("{author_str:.19}…")
        } else {
            format!("{author_str:<20}")
        };

        // Only show submodule if it changed from the previous entry
        let submodule_display = if prev_submodule.map(|s| s.name()) != i.1.map(|s| s.name()) {
            format!("{:^20}", i.1.map(|s| s.name()).unwrap_or_default())
        } else {
            format!("{:^20}", "")
        };
        prev_submodule = i.1;

        let lines = vec![Line::from(vec![
            // time
            Span::styled(i.0.time.clone(), Style::new().blue()),
            Span::raw(" "),
            // author
            Span::styled(author, Style::default().green()),
            Span::raw(" "),
            // submodule
            Span::styled(submodule_display, Style::default().gray()),
            Span::raw(" "),
            // message
            Span::styled(first_line, Style::default()),
        ])];
        list_items.push(ListItem::new(lines).style(Style::default()));
    }

    List::new(list_items)
        .highlight_style(
            Style::default()
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ")
}

pub fn run<'repo>(git_dir: PathBuf, log_entries: Vec<Item<'repo>>) -> Result<()> {
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut app = App::new(git_dir, log_entries);
    app.state.select(Some(0));

    let res = run_app(&mut terminal, app);

    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;

    res
}

enum Action {
    Quit,
    Select(usize),
    Continue,
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, mut app: App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        match handle_events(&mut app)? {
            Action::Quit => break,
            Action::Select(selected) => {
                let item = &app.items[selected];
                let current_dir = if let Some(submodule) = item.1 {
                    &submodule.git_dir()
                } else {
                    &app.git_dir
                };
                terminal.backend_mut().execute(LeaveAlternateScreen)?;
                disable_raw_mode()?;
                Command::new("git")
                    .arg("-c")
                    .arg("core.pager=less -RS +0")
                    .arg("show")
                    .arg(&item.0.commit_id)
                    .current_dir(current_dir)
                    .status()?;
                enable_raw_mode()?;
                terminal.backend_mut().execute(EnterAlternateScreen)?;
                terminal.clear()?;
            }
            Action::Continue => (),
        }
    }

    Ok(())
}

fn handle_events(app: &mut App) -> Result<Action> {
    if let Event::Key(key) = event::read()?
        && key.kind == event::KeyEventKind::Press
    {
        match key.code {
            KeyCode::Char('q') => return Ok(Action::Quit),
            KeyCode::Enter => {
                if let Some(selected) = app.state.selected() {
                    return Ok(Action::Select(selected));
                }
            }
            KeyCode::Char('j') | KeyCode::Down => app.next(),
            KeyCode::Char('k') | KeyCode::Up => app.previous(),
            KeyCode::PageDown => app.page_down(),
            KeyCode::PageUp => app.page_up(),
            KeyCode::Home => app.go_to_start(),
            KeyCode::End => app.go_to_end(),
            _ => {}
        }
    }

    Ok(Action::Continue)
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(100), Constraint::Min(1)].as_ref())
        .split(f.area());
    app.list_height = chunks[0].height.saturating_sub(2);

    f.render_stateful_widget(&app.list_items, chunks[0], &mut app.state);

    let status_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(100), Constraint::Min(4)].as_ref())
        .split(chunks[1]);

    let len = app.items.len();
    let selected = app.state.selected().unwrap_or(0);
    let item = &app.items[selected];
    let status = Line::from(format!(
        "{} - commit {} of {}",
        item.0.commit_id,
        selected + 1,
        len
    ))
    .style(Style::new().white().bold().on_light_blue());
    f.render_widget(status, status_layout[0]);
    let perc = Line::from(format!(
        "{}%",
        if len > 0 { selected * 100 / len } else { 0 }
    ))
    .style(Style::new().white().bold().on_light_blue());
    f.render_widget(perc, status_layout[1]);
}
