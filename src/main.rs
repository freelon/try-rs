use anyhow::Result;
use chrono::{DateTime, Local};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::{prelude::*, widgets::*};
use std::{fs, io, path::PathBuf, time::SystemTime};

// Modelo de dados (igual ao anterior)
#[derive(Clone)]
struct TryEntry {
    name: String,
    modified: SystemTime,
    score: i64,
}

// O estado da nossa TUI
struct App {
    query: String,                   // O que o usuário digitou
    all_entries: Vec<TryEntry>,      // Todos os diretórios encontrados
    filtered_entries: Vec<TryEntry>, // Diretórios filtrados pela busca
    selected_index: usize,           // Qual item está selecionado na lista
    should_quit: bool,               // Flag para sair do loop
    final_selection: Option<String>, // O resultado final (para o shell)
}

impl App {
    fn new(path: PathBuf) -> Self {
        let mut entries = Vec::new();
        if let Ok(read_dir) = fs::read_dir(path) {
            for entry in read_dir.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_dir() {
                        entries.push(TryEntry {
                            name: entry.file_name().to_string_lossy().to_string(),
                            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                            score: 0,
                        });
                    }
                }
            }
        }
        // Ordena inicial: mais recentes primeiro
        entries.sort_by(|a, b| b.modified.cmp(&a.modified));

        Self {
            query: String::new(),
            all_entries: entries.clone(),
            filtered_entries: entries,
            selected_index: 0,
            should_quit: false,
            final_selection: None,
        }
    }

    // Lógica de atualização do filtro
    fn update_search(&mut self) {
        let matcher = SkimMatcherV2::default();

        if self.query.is_empty() {
            self.filtered_entries = self.all_entries.clone();
        } else {
            self.filtered_entries = self
                .all_entries
                .iter()
                .filter_map(|entry| {
                    matcher.fuzzy_match(&entry.name, &self.query).map(|score| {
                        let mut e = entry.clone();
                        e.score = score;
                        e
                    })
                })
                .collect();

            // Ordena pelo score do fuzzy
            self.filtered_entries.sort_by(|a, b| b.score.cmp(&a.score));
        }
        self.selected_index = 0; // Reseta a seleção para o topo
    }
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    mut app: App,
) -> Result<Option<String>> {
    while !app.should_quit {
        terminal.draw(|f| {
            // 1. Layout: Divide a tela verticalmente (3 linhas pro input, resto pra lista)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(f.size());

            // 2. Widget de Input (Search)
            let search_text = Paragraph::new(app.query.clone())
                .style(Style::default().fg(Color::Yellow))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Buscar Experimento "),
                );
            f.render_widget(search_text, chunks[0]);

            // 3. Widget de Lista
            let items: Vec<ListItem> = app
                .filtered_entries
                .iter()
                .map(|entry| {
                    // Formata a data bonitinha
                    let date: DateTime<Local> = entry.modified.into();
                    let date_str = date.format("%Y-%m-%d %H:%M");

                    let content = Line::from(vec![
                        Span::raw(format!("{:<30}", entry.name)),
                        Span::styled(
                            format!("({})", date_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]);
                    ListItem::new(content)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" Resultados "))
                .highlight_style(
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");

            // O Ratatui precisa de um state para saber qual item renderizar como selecionado
            let mut state = ListState::default();
            state.select(Some(app.selected_index));

            f.render_stateful_widget(list, chunks[1], &mut state);
        })?;

        // 4. Tratamento de Eventos (Teclado)
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char(c) => {
                        app.query.push(c);
                        app.update_search();
                    }
                    KeyCode::Backspace => {
                        app.query.pop();
                        app.update_search();
                    }
                    KeyCode::Up => {
                        if app.selected_index > 0 {
                            app.selected_index -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if app.selected_index < app.filtered_entries.len().saturating_sub(1) {
                            app.selected_index += 1;
                        }
                    }
                    KeyCode::Enter => {
                        // Se a lista tiver itens, pega o selecionado.
                        // Se estiver vazia, usa o texto digitado (criar novo)
                        if !app.filtered_entries.is_empty() {
                            app.final_selection =
                                Some(app.filtered_entries[app.selected_index].name.clone());
                        } else if !app.query.is_empty() {
                            app.final_selection = Some(app.query.clone());
                        }
                        app.should_quit = true;
                    }
                    KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(app.final_selection)
}

fn main() -> Result<()> {
    // 1. Setup do diretório
    let home = dirs::home_dir().expect("Home não encontrado");
    let tries_dir = home.join("src/tries");
    fs::create_dir_all(&tries_dir)?;

    // 2. Setup do Terminal (Raw Mode)
    enable_raw_mode()?;
    // let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    // 3. Roda a aplicação
    let app = App::new(tries_dir.clone());
    let res = run_app(&mut terminal, app);

    // 4. Restaura o terminal (Muito importante!)
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // 5. Processa o resultado e imprime para o Shell
    if let Ok(Some(selection)) = res {
        let target_path = tries_dir.join(&selection);

        if target_path.exists() {
            // Pasta existe: retorna o comando para entrar nela
            println!("cd '{}'", target_path.to_string_lossy());
        } else {
            // Pasta nova: cria com data
            let now = Local::now();
            let date_prefix = now.format("%Y-%m-%d").to_string();
            // Evita duplicar a data se o usuário já digitou uma
            let new_name = if selection.starts_with(&date_prefix) {
                selection
            } else {
                format!("{}-{}", date_prefix, selection)
            };

            let new_path = tries_dir.join(&new_name);
            fs::create_dir_all(&new_path)?;
            println!("cd '{}'", new_path.to_string_lossy());
        }
    }

    Ok(())
}
