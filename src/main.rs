use arboard::Clipboard;
use csv::Writer;
use eframe::egui::{self, CentralPanel, ScrollArea, TextEdit};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::{fs::File, path::Path, sync::Arc, time::Duration};
use tokio::{sync::RwLock, task, time::sleep};

const DB_PATH: &str = "clipboard_history.db"; // SQLite database file
const EXPORT_PATH: &str = "clipboard_export.csv"; // Export file

#[derive(sqlx::FromRow)]
struct Data {
    id: i32,
    content: String,
}

#[tokio::main]
async fn main() {
    // Ensure database file exists
    if !Path::new(DB_PATH).exists() {
        println!("Database file not found, creating a new one...");
        std::fs::File::create(DB_PATH).expect("Failed to create database file");
    }

    // Initialize database connection
    let db_url = format!("sqlite://{}", DB_PATH);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    create_table(&pool).await;

    // Shared state for UI & clipboard monitoring
    let clipboard_history = Arc::new(RwLock::new(Vec::new()));

    let pool_clone = pool.clone();
    let history_clone = clipboard_history.clone();
    task::spawn(async move { monitor_clipboard(pool_clone, history_clone).await });

    // Run the GUI
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Clipboard History",
        options,
        Box::new(|_cc| {
            Box::new(ClipboardApp {
                pool,
                clipboard_history,
                local_history: Vec::new(),
                dark_mode: true,
            })
        }),
    )
    .expect("Failed to start GUI");
}

/// Creates the clipboard history table if it doesn't exist
async fn create_table(pool: &Pool<Sqlite>) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS clipboard_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool)
    .await
    .expect("Failed to create table");
}

/// Monitors clipboard and saves new content to the database
async fn monitor_clipboard(pool: Pool<Sqlite>, clipboard_history: Arc<RwLock<Vec<(i32, String)>>>) {
    let mut clipboard = Clipboard::new().expect("Failed to access clipboard");
    let mut last_clipboard_content = String::new();

    loop {
        if let Ok(content) = clipboard.get_text() {
            if content != last_clipboard_content {
                last_clipboard_content = content.clone();
                save_to_db(&pool, &content).await;
                update_ui_history(&pool, &clipboard_history).await;
            }
        }

        sleep(Duration::from_secs(1)).await;
    }
}

/// Saves clipboard content to the SQLite database
async fn save_to_db(pool: &Pool<Sqlite>, content: &str) {
    sqlx::query("INSERT INTO clipboard_history (content) VALUES (?1)")
        .bind(content)
        .execute(pool)
        .await
        .expect("Failed to insert clipboard data into database");
}

/// Fetches clipboard history from the database and updates the UI state
async fn update_ui_history(
    pool: &Pool<Sqlite>,
    clipboard_history: &Arc<RwLock<Vec<(i32, String)>>>,
) {
    let rows: Vec<Data> =
        sqlx::query_as("SELECT id, content FROM clipboard_history ORDER BY id DESC LIMIT 50")
            .fetch_all(pool)
            .await
            .expect("Failed to fetch clipboard history");

    let mut history = clipboard_history.write().await;
    *history = rows.into_iter().map(|row| (row.id, row.content)).collect();
}

/// Monitors clipboard and saves new content to the database
async fn delete_entry(
    pool: &Pool<Sqlite>,
    id: i32,
    clipboard_history: Arc<RwLock<Vec<(i32, String)>>>,
) {
    sqlx::query("DELETE FROM clipboard_history WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await
        .expect("Failed to delete clipboard entry");

    update_ui_history(pool, &clipboard_history).await;
}

/// Exports clipboard history to a CSV file
async fn export_to_csv(pool: &Pool<Sqlite>) {
    let rows: Vec<Data> =
        sqlx::query_as("SELECT id, content FROM clipboard_history ORDER BY id DESC")
            .fetch_all(pool)
            .await
            .expect("Failed to fetch clipboard history for export");

    let mut writer =
        Writer::from_writer(File::create(EXPORT_PATH).expect("Failed to create CSV file"));

    for row in rows {
        writer
            .write_record(&[row.content])
            .expect("Failed to write to CSV");
    }

    writer.flush().expect("Failed to flush CSV writer");
    println!("Clipboard history exported to {}", EXPORT_PATH);
}

/// **Clipboard History App UI**
struct ClipboardApp {
    pool: Pool<Sqlite>,
    clipboard_history: Arc<RwLock<Vec<(i32, String)>>>,
    local_history: Vec<(i32, String)>,
    search_query: String, // Stores search input
    dark_mode: bool,
}

impl eframe::App for ClipboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let pool_clone = self.pool.clone();
        let history_clone_task = self.clipboard_history.clone();
        let history_clone_ui = self.clipboard_history.clone();

        // **Update history in a background task**
        task::spawn(async move {
            update_ui_history(&pool_clone, &history_clone_task).await;
        });

        // **Try reading history without blocking UI**
        if let Ok(history) = history_clone_ui.try_read() {
            self.local_history = history.clone();
        }

        // **UI Rendering**
        egui::CentralPanel::default().show(ctx, |_ui| {
            if self.dark_mode {
                ctx.set_visuals(egui::Visuals::dark());
            } else {
                ctx.set_visuals(egui::Visuals::light());
            }

            egui::TopBottomPanel::top("header").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Theme:");
                    if ui
                        .button(if self.dark_mode { "🌙" } else { "☀️" })
                        .clicked()
                    {
                        self.dark_mode = !self.dark_mode;
                    }
                    if ui.button("📂 Export to CSV").clicked() {
                        let pool_clone = self.pool.clone();
                        task::spawn(async move {
                            export_to_csv(&pool_clone).await;
                        });
                    }
                });
            });

            CentralPanel::default().show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    for (id, entry) in &self.local_history {
                        ui.horizontal(|ui| {
                            if ui.button("📋 Copy").clicked() {
                                let mut clipboard =
                                    Clipboard::new().expect("Failed to access clipboard");
                                clipboard
                                    .set_text(entry.clone())
                                    .expect("Failed to copy to clipboard");
                            }
                            if ui.button("❌ Delete").clicked() {
                                let id_copy = *id;
                                let pool_clone = self.pool.clone();
                                let history_clone = self.clipboard_history.clone();
                                task::spawn(async move {
                                    delete_entry(&pool_clone, id_copy, history_clone).await;
                                });
                            }
                            ui.add(
                                TextEdit::singleline(&mut entry.clone())
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    }
                });
            });
        });

        // **Request repaint to keep UI updated**
        ctx.request_repaint();
    }
}
