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
use serde::Deserialize;
use std::process::Stdio;
use std::{fs, io, path::PathBuf, time::SystemTime};

#[derive(Clone, Copy, PartialEq)]
enum AppMode {
    Normal,
    DeleteConfirm,
}

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
    mode: AppMode,
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
            mode: AppMode::Normal,
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

    // NOVO MÉTODO: Função para apagar o item selecionado
    fn delete_selected(&mut self, base_path: &std::path::Path) {
        if let Some(entry) = self.filtered_entries.get(self.selected_index) {
            let path_to_remove = base_path.join(&entry.name);

            // Tenta remover o diretório
            if fs::remove_dir_all(&path_to_remove).is_ok() {
                // Remove da lista em memória 'all_entries'
                self.all_entries.retain(|e| e.name != entry.name);
                // Atualiza a pesquisa para refrescar a lista filtrada
                self.update_search();
            }
        }
        // Volta ao modo normal
        self.mode = AppMode::Normal;
    }
}

fn draw_popup(f: &mut Frame, title: &str, message: &str) {
    let area = f.size();

    // 1. Define uma área no centro (60% de largura, 20% de altura)
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(3), // Altura do popup
            Constraint::Percentage(40),
        ])
        .split(area);

    let popup_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60), // Largura do popup
            Constraint::Percentage(20),
        ])
        .split(popup_layout[1])[1];

    // 2. Limpa a área do popup (para não ver o texto de trás misturado)
    f.render_widget(Clear, popup_area);

    // 3. Cria o bloco com borda vermelha (alerta)
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::DarkGray)); // Fundo cinza escuro

    let paragraph = Paragraph::new(message)
        .block(block)
        .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);

    f.render_widget(paragraph, popup_area);
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    mut app: App,
) -> Result<Option<String>> {
    // Precisamos do caminho base para poder deletar
    // (Poderíamos ter guardado na struct App, mas vamos pegar do contexto aqui)
    let home = dirs::home_dir().expect("Home não encontrado");
    let tries_dir = home.join("src/tries");

    while !app.should_quit {
        terminal.draw(|f| {
            // --- DESENHO DA LISTA (Normal) ---
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .split(f.size());

            let search_text = Paragraph::new(app.query.clone())
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().borders(Borders::ALL).title(" Search "));
            f.render_widget(search_text, chunks[0]);

            let items: Vec<ListItem> = app
                .filtered_entries
                .iter()
                .map(|entry| {
                    let date: DateTime<Local> = entry.modified.into();
                    let date_str = date.format("%Y-%m-%d %H:%M");
                    let content = Line::from(vec![
                        Span::raw(format!("📁{:<30}", entry.name)),
                        Span::styled(
                            format!("({})", date_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]);
                    ListItem::new(content)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" Folders "))
                .highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("→ ");

            let mut state = ListState::default();
            state.select(Some(app.selected_index));
            f.render_stateful_widget(list, chunks[1], &mut state);

            // --- NOVO: Widget de Rodapé (Help) ---
            // Usamos 'Line' e 'Span' para poder estilizar partes do texto (negrito nas teclas)
            let help_text = Line::from(vec![
                Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": Navigate  "),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": Select  "),
                Span::styled("Ctrl-D", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": Delete  "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": Cancel"),
            ]);

            let help_message = Paragraph::new(help_text)
                .style(Style::default().fg(Color::DarkGray)) // Cor discreta
                .alignment(Alignment::Center); // Centralizado (opcional, pode remover para ficar à esquerda)

            f.render_widget(help_message, chunks[2]);

            // --- DESENHO DO POPUP (Se estiver no modo DeleteConfirm) ---
            if app.mode == AppMode::DeleteConfirm {
                if let Some(selected) = app.filtered_entries.get(app.selected_index) {
                    let msg = format!("Apagar '{}'? (y/n)", selected.name);
                    draw_popup(f, " ATENÇÃO ", &msg);
                }
            }
        })?;

        // --- TRATAMENTO DE TECLAS ---
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                // O comportamento depende do modo
                match app.mode {
                    AppMode::Normal => match key.code {
                        KeyCode::Char(c) => {
                            // Ctrl+D para deletar
                            if c == 'd' && key.modifiers.contains(event::KeyModifiers::CONTROL) {
                                // Só entra no modo delete se houver algo selecionado
                                if !app.filtered_entries.is_empty() {
                                    app.mode = AppMode::DeleteConfirm;
                                }
                            } else {
                                app.query.push(c);
                                app.update_search();
                            }
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
                            if !app.filtered_entries.is_empty() {
                                app.final_selection =
                                    Some(app.filtered_entries[app.selected_index].name.clone());
                            } else if !app.query.is_empty() {
                                app.final_selection = Some(app.query.clone());
                            }
                            app.should_quit = true;
                        }
                        KeyCode::Esc => app.should_quit = true,
                        _ => {}
                    },

                    AppMode::DeleteConfirm => match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            // Confirmou!
                            app.delete_selected(&tries_dir);
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            // Cancelou
                            app.mode = AppMode::Normal;
                        }
                        _ => {} // Ignora outras teclas no popup
                    },
                }
            }
        }
    }

    Ok(app.final_selection)
}

// Representação do nosso arquivo TOML
#[derive(Deserialize)]
struct Config {
    tries_path: Option<String>,
}

// Função auxiliar para substituir "~" pelo caminho real da home
fn expand_path(path_str: &str) -> PathBuf {
    if path_str.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            // Remove o "~/" (primeiros 2 chars) e junta com o home
            return home.join(&path_str[2..]);
        }
    }
    PathBuf::from(path_str)
}

fn get_configuration_path() -> PathBuf {
    // 1. Tenta achar o diretório de config padrão (~/.config)
    let config_dir = dirs::config_dir().unwrap_or_else(|| {
        // Fallback se não achar
        dirs::home_dir()
            .expect("Home não encontrada")
            .join(".config")
    });

    // 2. Monta o caminho: ~/.config/try-rs/config.toml
    let config_file = config_dir.join("try-rs").join("config.toml");

    // 3. Define o padrão antigo (fallback)
    let default_path = dirs::home_dir()
        .expect("Home não encontrada")
        .join("src/tries");

    // 4. Se o arquivo existe, tenta ler
    if config_file.exists() {
        if let Ok(contents) = fs::read_to_string(&config_file) {
            if let Ok(config) = toml::from_str::<Config>(&contents) {
                if let Some(path_str) = config.tries_path {
                    return expand_path(&path_str);
                }
            }
        }
    }

    // Se nada der certo ou não tiver config, retorna o padrão
    default_path
}

fn main() -> Result<()> {
    // 1. Setup do diretório
    // let home = dirs::home_dir().expect("Home not found");
    // let tries_dir = home.join("src/tries");
    // fs::create_dir_all(&tries_dir)?;
    //
    let tries_dir = get_configuration_path();

    // Garante que o diretório existe (seja o do config ou o padrão)
    if !tries_dir.exists() {
        fs::create_dir_all(&tries_dir)?;
    }

    // 2. Verifica argumentos da linha de comando
    let args: Vec<String> = std::env::args().collect();

    // A variável 'selection' vai guardar o nome ou URL escolhido.
    // Pode vir dos argumentos (CLI) ou da interface (TUI).
    let selection_result: Option<String>;

    if args.len() > 1 {
        // MODO CLI: O usuário passou um argumento (ex: try-rs https://...)
        // Pulamos a interface gráfica totalmente.
        selection_result = Some(args[1].clone());
    } else {
        // MODO TUI: Nenhum argumento, abre a interface visual.

        enable_raw_mode()?;
        let mut stderr = io::stderr();
        execute!(stderr, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stderr);
        let mut terminal = Terminal::new(backend)?;

        let app = App::new(tries_dir.clone());
        // Roda o app e captura o resultado
        selection_result = run_app(&mut terminal, app)?;

        // Restaura o terminal
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
    }

    // 3. Processa o resultado (Comum para os dois modos)
    if let Some(selection) = selection_result {
        let target_path = tries_dir.join(&selection);

        // CASO 1: A pasta já existe? Entra nela.
        if target_path.exists() {
            println!("cd '{}'", target_path.to_string_lossy());
        } else {
            // CASO 2: É um URL Git? Clona!
            if is_git_url(&selection) {
                let repo_name = extract_repo_name(&selection);

                let now = Local::now();
                let date_prefix = now.format("%Y-%m-%d").to_string();
                let folder_name = format!("{}-{}", date_prefix, repo_name);
                let new_path = tries_dir.join(&folder_name);

                eprintln!("A clonar {} para {}...", selection, folder_name);

                let status = std::process::Command::new("git")
                    .arg("clone")
                    .arg(&selection)
                    .arg(&new_path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .status();

                match status {
                    Ok(s) if s.success() => {
                        println!("cd '{}'", new_path.to_string_lossy());
                    }
                    _ => {
                        eprintln!("Error: Failed to clone the repository.");
                    }
                }
            } else {
                // CASO 3: Cria pasta vazia
                let now = Local::now();
                let date_prefix = now.format("%Y-%m-%d").to_string();

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
    }

    Ok(())
}

// Verifica se a string parece um link Git
fn is_git_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("git@")
        || s.starts_with("ssh://")
}

// Extrai um nome limpo do repositório (ex: "github.com/tobi/try.git" -> "try")
fn extract_repo_name(url: &str) -> String {
    // Remove o sufixo .git se existir
    let clean_url = url.trim_end_matches(".git");

    // Pega a última parte após a barra '/' ou dois pontos ':' (comum em ssh)
    if let Some(last_part) = clean_url.rsplit(|c| c == '/' || c == ':').next() {
        if !last_part.is_empty() {
            return last_part.to_string();
        }
    }
    // Nome genérico caso falhe a deteção
    "repo-clonado".to_string()
}
