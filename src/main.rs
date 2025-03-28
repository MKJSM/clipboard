use arboard::Clipboard;
use csv::Writer;
use eframe::egui::{self, CentralPanel, ScrollArea, TextEdit, TopBottomPanel, Visuals};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::{
    env,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
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
    setup_autostart();
    run_clipboard_monitor().await;
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
    local_history: Vec<(i32, String)>, // Stores history for UI updates
    filtered_history: Vec<(i32, String)>, // Stores filtered search results
    search_query: String,              // Stores search input
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

        // **Read history & filter results**
        if let Ok(history) = history_clone_ui.try_read() {
            self.local_history = history.clone();
            self.filtered_history = self
                .local_history
                .iter()
                .filter(|(_, content)| {
                    content
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase())
                })
                .cloned()
                .collect();
        }

        // **Apply Theme**
        ctx.set_visuals(if self.dark_mode {
            Visuals::dark()
        } else {
            Visuals::light()
        });

        // **Header Panel (Buttons + Search Bar)**
        TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // **Theme Toggle Button**
                if ui
                    .button(if self.dark_mode {
                        "🌙 Dark"
                    } else {
                        "☀️ Light"
                    })
                    .clicked()
                {
                    self.dark_mode = !self.dark_mode; // Toggle state
                }

                if ui.button("📂 Export to CSV").clicked() {
                    let pool_clone = self.pool.clone();
                    task::spawn(async move {
                        export_to_csv(&pool_clone).await;
                    });
                }

                // **Search Bar**
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.search_query)
                            .hint_text("🔍 Search clipboard..."),
                    );
                });
            });
        });

        // **Main Content (Filtered clipboard history with scroll)**
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                for (id, entry) in &self.filtered_history {
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
                            TextEdit::singleline(&mut entry.clone()).desired_width(f32::INFINITY),
                        );
                    });
                }
            });
        });

        // **Request repaint to keep UI updated**
        ctx.request_repaint();
    }
}

/// 📌 Automatically register clipboard manager to run at startup
fn setup_autostart() {
    let binary_path = env::current_exe().expect("Failed to get binary path");

    #[cfg(target_os = "linux")]
    setup_autostart_linux(&binary_path);

    #[cfg(target_os = "macos")]
    setup_autostart_macos(&binary_path);

    #[cfg(target_os = "windows")]
    setup_autostart_windows(&binary_path);
}

/// 📌 Linux: Create a `systemd` service for auto-start#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
fn setup_autostart_linux(binary_path: &PathBuf) {
    use dirs::home_dir;
    use std::fs;
    use std::process::Command;

    let systemd_user_dir = home_dir().unwrap().join(".config/systemd/user");

    // Ensure the directory exists
    if !systemd_user_dir.exists() {
        fs::create_dir_all(&systemd_user_dir).expect("Failed to create systemd user directory");
    }

    let service_file = systemd_user_dir.join("clipboard-manager.service");

    let service_content = format!(
        "[Unit]
        Description=Clipboard Manager
        After=network.target

        [Service]
        ExecStart={}
        Restart=always
        Environment=DISPLAY=:0

        [Install]
        WantedBy=default.target",
        binary_path.display()
    );

    fs::write(&service_file, service_content).expect("Failed to write systemd service");

    // Reload systemd daemon to recognize the new service
    Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output()
        .expect("Failed to reload systemd daemon");

    // Enable and start the service
    Command::new("systemctl")
        .args(["--user", "enable", "clipboard-manager"])
        .output()
        .expect("Failed to enable systemd service");

    Command::new("systemctl")
        .args(["--user", "start", "clipboard-manager"])
        .output()
        .expect("Failed to start clipboard-manager");
}

/// 📌 macOS: Create a `launchd` plist for auto-start
#[cfg(target_os = "macos")]
fn setup_autostart_macos(binary_path: &PathBuf) {
    let plist_file = home_dir()
        .unwrap()
        .join("Library/LaunchAgents/com.clipboard.manager.plist");

    let plist_content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
    <dict>
        <key>Label</key>
        <string>com.clipboard.manager</string>
        <key>ProgramArguments</key>
        <array>
            <string>{}</string>
        </array>
        <key>RunAtLoad</key>
        <true/>
        <key>KeepAlive</key>
        <true/>
    </dict>
</plist>",
        binary_path.display()
    );

    fs::write(&plist_file, plist_content).expect("Failed to write macOS launchd plist");

    Command::new("launchctl")
        .args(["load", plist_file.to_str().unwrap()])
        .output()
        .expect("Failed to load launchd plist");

    Command::new("launchctl")
        .args(["start", "com.clipboard.manager"])
        .output()
        .expect("Failed to start clipboard-manager");
}

/// 📌 Windows: Create a Task Scheduler entry
#[cfg(target_os = "windows")]
fn setup_autostart_windows(binary_path: &PathBuf) {
    let task_name = "ClipboardManager";

    Command::new("schtasks")
        .args(&[
            "/Create",
            "/F",
            "/SC",
            "ONLOGON",
            "/TN",
            task_name,
            "/TR",
            binary_path.to_str().unwrap(),
            "/RL",
            "LOW",
        ])
        .output()
        .expect("Failed to create Windows scheduled task");
}

/// 📌 Runs the clipboard monitoring (Replace with your actual logic)
async fn run_clipboard_monitor() {
    println!("📋 Clipboard Manager is running in the background...");
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
                search_query: String::new(),
                filtered_history: Vec::new(),
                dark_mode: true,
            })
        }),
    )
    .expect("Failed to start GUI");
}
