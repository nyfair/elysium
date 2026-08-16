use anyhow::Result;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::Position;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use std::io::stdout;
use std::time::Duration;

use crate::{Args, GameType, k};
use crate::script_engine;
use crate::worker;

const PARAMS: [&str; 5] = ["strategy", "combo", "timeout", "boost", "turn"];
const PARAM_HELP: [&str; 5] = [
    "开局策略",
    "循环策略",
    "",
    "使用委托手册加成（1-4）/ 不使用 0",
    "",
];

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

fn hl_style(focused: bool) -> Style {
    if focused {
        Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(MUTED).fg(Color::White)
    }
}

fn panel(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { ACCENT } else { MUTED }))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
}

#[derive(PartialEq, Clone, Copy)]
enum Focus {
    Game,
    Task,
    Param,
}

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Config,
    Custom,
    Running,
}

struct App {
    page: Page,
    games: Vec<GameType>,
    selected_game: usize,
    tasks: Vec<String>,
    selected_task: usize,
    focus: Focus,
    param_idx: usize,
    boost: String,
    turn: String,
    timeout: String,
    combo: String,
    strategy: String,
    default_boost: String,
    default_turn: String,
    default_timeout: String,
    game_rect: Option<Rect>,
    task_rect: Option<Rect>,
    param_rect: Option<Rect>,
    log_scroll: usize,
    notice: Option<String>,
    script_meta: Option<script_engine::ScriptMeta>,
    custom: bool,
    custom_script: String,
    custom_rect: Option<Rect>,
    worker: Option<worker::Worker>,
    quit: bool,
}

impl App {
    fn new(args: &Args) -> Self {
        let games = vec![
            #[cfg(feature = "dna")]
            GameType::Dna,
            #[cfg(feature = "nte")]
            GameType::Nte,
        ];
        let tasks = scan_tasks(&games[0]);
        let mut app = Self {
            page: Page::Config,
            games,
            selected_game: 0,
            tasks,
            selected_task: 0,
            focus: Focus::Game,
            param_idx: 0,
            boost: args.boost.to_string(),
            turn: args.turn.to_string(),
            timeout: args.timeout.to_string(),
            combo: args.combo.clone(),
            strategy: args.strategy.clone(),
            default_boost: args.boost.to_string(),
            default_turn: args.turn.to_string(),
            default_timeout: args.timeout.to_string(),
            game_rect: None,
            task_rect: None,
            param_rect: None,
            log_scroll: 0,
            notice: None,
            script_meta: None,
            custom: false,
            custom_script: String::new(),
            custom_rect: None,
            worker: None,
            quit: false,
        };
        app.load_script_meta();
        app
    }

    fn selected_game(&self) -> GameType {
        self.games[self.selected_game].clone()
    }

    fn refresh_tasks(&mut self) {
        self.tasks = scan_tasks(&self.selected_game());
        if self.tasks.is_empty() {
            self.selected_task = 0;
        } else if self.selected_task >= self.tasks.len() {
            self.selected_task = self.tasks.len() - 1;
        }
        self.load_script_meta();
    }

    fn load_script_meta(&mut self) {
        self.script_meta = self
            .tasks
            .get(self.selected_task)
            .and_then(|t| {
                std::fs::read_to_string(crate::script_path(self.selected_game().name(), t)).ok()
            })
            .map(|s| script_engine::ScriptMeta::parse(&s));
    }

    fn build_args(&self) -> Args {
        Args {
            game: Some(self.selected_game()),
            task: self.tasks.get(self.selected_task).cloned(),
            plan: "".to_string(),
            boost: self.boost.parse().unwrap_or(0),
            turn: self.turn.parse().unwrap_or(99),
            timeout: self.timeout.parse().unwrap_or(0),
            combo: self.combo.clone(),
            strategy: self.strategy.clone(),
        }
    }
}

fn scan_tasks(game: &GameType) -> Vec<String> {
    let mut tasks = Vec::new();
    for dir in [format!("{}/scripts", game.name()), format!("user-{}-scripts", game.name())] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                if p.extension().map(|x| x == "rhai").unwrap_or(false) {
                    if let Some(s) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) {
                        if !tasks.contains(&s) {
                            tasks.push(s);
                        }
                    }
                }
            }
        }
    }
    tasks
}

pub fn run(args: &Args) -> Result<()> {
    let mut terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture)?;
    let result = run_app(&mut terminal, args);
    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut ratatui::DefaultTerminal, args: &Args) -> Result<()> {
    let mut app = App::new(args);
    while !app.quit {
        terminal.draw(|f| draw(f, &mut app))?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        app.quit = true;
                    } else {
                        handle_key(&mut app, key);
                    }
                }
                Event::Mouse(me) => handle_mouse(&mut app, me),
                _ => {}
            }
        }
        check_worker(&mut app);
    }
    Ok(())
}

fn draw(f: &mut Frame, app: &mut App) {
    match app.page {
        Page::Config => draw_config(f, app),
        Page::Custom => draw_custom(f, app),
        Page::Running => draw_running(f, app),
    }
}

fn check_worker(app: &mut App) {
    if app.page != Page::Running {
        return;
    }
    let finished = match &app.worker {
        Some(w) => !k!(w.shared).running,
        None => false,
    };
    if finished {
        if let Some(w) = app.worker.take() {
            let err = k!(w.shared).error.clone();
            app.notice = err;
            let _ = w.thread.join();
        }
        app.custom = false;
        app.page = Page::Config;
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    match app.page {
        Page::Config => handle_config_key(app, key),
        Page::Custom => handle_custom_key(app, key),
        Page::Running => handle_running_key(app, key),
    }
}

fn handle_custom_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('\u{1b}') => app.page = Page::Config,
        KeyCode::F(5) => {
            if !app.custom_script.is_empty() {
                let args = app.build_args();
                let game = args.game.clone().unwrap();
                let script = app.custom_script.clone();
                match worker::spawn_custom(game, script, args) {
                    Ok(w) => {
                        app.worker = Some(w);
                        app.custom = true;
                        app.page = Page::Running;
                    }
                    Err(e) => eprintln!("任务启动失败：{e}"),
                }
            }
        }
        KeyCode::Enter => app.custom_script.push('\n'),
        KeyCode::Backspace => {
            app.custom_script.pop();
        }
        KeyCode::F(6) => app.custom_script.clear(),
        KeyCode::Char(c) => app.custom_script.push(c),
        _ => {}
    }
}

fn handle_config_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('\u{1b}') => app.quit = true,
        KeyCode::Up => match app.focus {
            Focus::Game => {
                if app.selected_game > 0 {
                    app.selected_game -= 1;
                    app.refresh_tasks();
                }
            }
            Focus::Task => {
                if !app.tasks.is_empty() {
                    app.selected_task = app.selected_task.saturating_sub(1);
                    app.load_script_meta();
                }
            }
            Focus::Param => app.param_idx = app.param_idx.saturating_sub(1),
        },
        KeyCode::Down => match app.focus {
            Focus::Game => {
                if app.selected_game + 1 < app.games.len() {
                    app.selected_game += 1;
                    app.refresh_tasks();
                }
            }
            Focus::Task => {
                if app.selected_task + 1 < app.tasks.len() {
                    app.selected_task += 1;
                    app.load_script_meta();
                }
            }
            Focus::Param => {
                if app.param_idx + 1 < PARAMS.len() {
                    app.param_idx += 1;
                }
            }
        },
        KeyCode::Left => match app.focus {
            Focus::Game => {}
            Focus::Task => app.focus = Focus::Game,
            Focus::Param => app.focus = Focus::Task,
        },
        KeyCode::Right => match app.focus {
            Focus::Game => app.focus = Focus::Task,
            Focus::Task => app.focus = Focus::Param,
            Focus::Param => {}
        },
        KeyCode::Tab => match app.focus {
            Focus::Game => app.focus = Focus::Task,
            Focus::Task => app.focus = Focus::Param,
            Focus::Param => app.focus = Focus::Game,
        },
        KeyCode::Backspace => match app.param_idx {
            0 => { app.strategy.pop(); }
            1 => { app.combo.pop(); }
            2 => { app.timeout.pop(); }
            3 => { app.boost.pop(); }
            _ => { app.turn.pop(); }
        },
        KeyCode::Enter => {
            app.notice = None;
            if !app.tasks.is_empty() {
                let args = app.build_args();
                let game = args.game.clone().unwrap();
                let task = args.task.clone().unwrap();
                match worker::spawn(game, task, args) {
                    Ok(w) => {
                        app.worker = Some(w);
                        app.page = Page::Running;
                    }
                    Err(e) => eprintln!("任务启动失败：{e}"),
                }
            }
        }
        KeyCode::Char(c) if app.focus == Focus::Param => match app.param_idx {
            0 => app.strategy.push(c),
            1 => app.combo.push(c),
            2 if c.is_ascii_digit() => {
                if app.timeout == app.default_timeout {
                    app.timeout.clear();
                }
                app.timeout.push(c);
            }
            3 if c.is_ascii_digit() => {
                if app.boost == app.default_boost {
                    app.boost.clear();
                }
                app.boost.push(c);
            }
            4 if c.is_ascii_digit() => {
                if app.turn == app.default_turn {
                    app.turn.clear();
                }
                app.turn.push(c);
            }
            _ => {}
        },
        _ => {}
    }
}

fn handle_running_key(app: &mut App, key: KeyEvent) {
    let Some(w) = &app.worker else { return };
    match key.code {
        KeyCode::Esc | KeyCode::Char('\u{1b}') => {
            w.exit.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        KeyCode::Char('p') => {
            let mut p = k!(&w.state.pause);
            *p = !*p;
        }
        KeyCode::Char('r') => {
            w.reset.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        _ => {}
    }
}

fn handle_mouse(app: &mut App, me: MouseEvent) {
    match me.kind {
        MouseEventKind::ScrollUp => {
            if app.page == Page::Running {
                app.log_scroll += 3;
            }
        }
        MouseEventKind::ScrollDown => {
            if app.page == Page::Running {
                app.log_scroll = app.log_scroll.saturating_sub(3);
            }
        }
        MouseEventKind::Down(MouseButton::Left)
            if app.page == Page::Config => {
                handle_config_click(app, me.row, me.column);
            }
        _ => {}
    }
}

fn handle_config_click(app: &mut App, row: u16, col: u16) {
    let pos = Position::new(col, row);
    if let Some(r) = app.game_rect
        && r.contains(pos) && row > r.y {
            let idx = (row - r.y - 1) as usize;
            if idx < app.games.len() {
                app.selected_game = idx;
                app.focus = Focus::Game;
                app.refresh_tasks();
            }
            return;
        }
    if let Some(r) = app.task_rect
        && r.contains(pos) && row > r.y {
            let idx = (row - r.y - 1) as usize;
            if idx < app.tasks.len() {
                app.selected_task = idx;
                app.focus = Focus::Task;
                app.load_script_meta();
            }
            return;
        }
    if let Some(r) = app.param_rect
        && r.contains(pos) && row > r.y {
            let idx = (row - r.y - 1) as usize;
            if idx < PARAMS.len() {
                app.param_idx = idx;
                app.focus = Focus::Param;
            }
        }
    if let Some(r) = app.custom_rect
        && r.contains(pos) {
            app.page = Page::Custom;
        }
}

fn draw_config(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(30), Constraint::Percentage(45)])
        .split(chunks[0]);

    let items: Vec<ListItem> = app.games.iter().map(|g| ListItem::new(g.display())).collect();
    let mut gs = ListState::default();
    gs.select(Some(app.selected_game));
    let focused = app.focus == Focus::Game;
    let list = List::new(items)
        .block(panel(" 游戏 ", focused))
        .highlight_style(hl_style(focused));
    f.render_stateful_widget(list, cols[0], &mut gs);

    let items: Vec<ListItem> = app.tasks.iter().map(|t| ListItem::new(t.clone())).collect();
    let mut ts = ListState::default();
    if !app.tasks.is_empty() {
        ts.select(Some(app.selected_task));
    }
    let focused = app.focus == Focus::Task;
    let list = List::new(items)
        .block(panel(" 任务 ", focused))
        .highlight_style(hl_style(focused));
    f.render_stateful_widget(list, cols[1], &mut ts);

    app.game_rect = Some(cols[0]);
    app.task_rect = Some(cols[1]);
    app.param_rect = Some(cols[2]);

    let values = [&app.strategy, &app.combo, &app.timeout, &app.boost, &app.turn];
    let mut lines: Vec<Line> = Vec::new();
    for (i, name) in PARAMS.iter().enumerate() {
        let selected = app.focus == Focus::Param && app.param_idx == i;
        let style = if selected {
            hl_style(true)
        } else {
            Style::default()
        };
        let name_style = if selected {
            style
        } else {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{name:<10}"), name_style),
            Span::styled(values[i].clone(), style),
        ]));
    }
    let p = Paragraph::new(lines).block(panel(" 参数 ", app.focus == Focus::Param));

    let param_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(cols[2]);
    f.render_widget(p, param_chunks[0]);

    let custom_p = Paragraph::new(
        Span::styled(" [自定义脚本]", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(custom_p, param_chunks[1]);

    app.param_rect = Some(param_chunks[0]);
    app.custom_rect = Some(param_chunks[1]);

    let info = if app.focus == Focus::Param {
        PARAM_HELP[app.param_idx].to_string()
    } else if let Some(m) = &app.script_meta {
        let mut s = String::new();
        if !m.desc.is_empty() {
            s.push_str(&m.desc);
        }
        if !m.author.is_empty() {
            s.push_str(&format!("   作者：{}", m.author));
        }
        s
    } else {
        String::new()
    };
    let help_p = Paragraph::new(info).style(Style::default().fg(Color::Yellow));
    f.render_widget(help_p, chunks[1]);

    let notice = app.notice.clone().unwrap_or_default();
    let notice_p = Paragraph::new(notice).style(Style::default().fg(Color::Red));
    f.render_widget(notice_p, chunks[2]);

    let hint = Paragraph::new(" ↑↓ 选择    ←→/Tab 切换列    Enter 开始    Esc 退出 ")
        .style(Style::default().fg(MUTED));
    f.render_widget(hint, chunks[3]);
}

fn draw_custom(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let p = Paragraph::new(app.custom_script.clone())
        .block(panel(" 自定义脚本（调试用）", true))
        .wrap(Wrap { trim: false });
    f.render_widget(p, chunks[0]);

    let hint = Paragraph::new(" 输入脚本后 F5 执行    F6 清空    Esc 返回 ")
        .style(Style::default().fg(MUTED));
    f.render_widget(hint, chunks[1]);
}

fn draw_running(f: &mut Frame, app: &mut App) {
    let Some(w) = &app.worker else { return };
    let shared = k!(w.shared);
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let (status, color) = if let Some(err) = &shared.error {
        (format!("错误: {err}"), Color::Red)
    } else if *k!(&w.state.pause) {
        ("暂停中".to_string(), Color::Yellow)
    } else {
        ("运行中".to_string(), Color::Green)
    };
    let elapsed = shared.started_at.elapsed();
    let title = if app.custom {
        format!(
            " 自定义脚本   状态:{}   时长 {:02}:{:02} ",
            status,
            elapsed.as_secs() / 60,
            elapsed.as_secs() % 60
        )
    } else {
        format!(
            " {} / {}   状态:{}   时长 {:02}:{:02} ",
            app.selected_game().title(),
            app.tasks.get(app.selected_task).cloned().unwrap_or_default(),
            status,
            elapsed.as_secs() / 60,
            elapsed.as_secs() % 60
        )
    };
    let status_line = Paragraph::new(title).style(Style::default().fg(color).add_modifier(Modifier::BOLD));
    f.render_widget(status_line, chunks[0]);

    let lines: Vec<Line> = shared.logs.iter().map(|l| Line::from(l.clone())).collect();
    let log_area = chunks[1];
    let visible = (log_area.height as usize).saturating_sub(2);
    let max_start = lines.len().saturating_sub(visible);
    let scroll = app.log_scroll.min(max_start);
    let start = max_start - scroll;
    let view: Vec<Line> = lines[start..].to_vec();
    let p = Paragraph::new(view)
        .block(panel(" 日志 ", true))
        .wrap(Wrap { trim: false });
    f.render_widget(p, log_area);

    let hint = if app.custom {
        Paragraph::new(" 脚本运行中（若死循环请按 Ctrl+C 退出） ")
            .style(Style::default().fg(MUTED))
    } else {
        Paragraph::new(" P 暂停/继续    R 重置    Esc 停止任务 ")
            .style(Style::default().fg(MUTED))
    };
    f.render_widget(hint, chunks[2]);
}
