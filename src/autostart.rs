use dirs::home_dir;
use std::fs;
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "linux")]
pub fn setup_autostart_linux(binary_path: &Path) {
    let systemd_path = home_dir().unwrap().join(".config/systemd/user");

    // Ensure the systemd directory exists
    if !systemd_path.exists() {
        fs::create_dir_all(&systemd_path).expect("Failed to create systemd directory");
    }

    let service_file = systemd_path.join("clipboard-manager.service");

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

    // Enable & Start the service
    Command::new("systemctl")
        .args(["--user", "enable", "clipboard-manager"])
        .output()
        .expect("Failed to enable systemd service");

    Command::new("systemctl")
        .args(["--user", "start", "clipboard-manager"])
        .output()
        .expect("Failed to start clipboard-manager");

    println!("Clipboard Manager set to auto-start on boot.");
}

#[cfg(target_os = "windows")]
pub fn setup_autostart_windows(binary_path: &Path) {
    let startup_folder = home_dir()
        .unwrap()
        .join("AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Startup");

    let shortcut_path = startup_folder.join("Clipboard Manager.lnk");

    let script_content = format!(
        "$WScriptShell = New-Object -ComObject WScript.Shell
        $Shortcut = $WScriptShell.CreateShortcut('{}')
        $Shortcut.TargetPath = '{}'
        $Shortcut.Save()",
        shortcut_path.display(),
        binary_path.display()
    );

    let script_path = startup_folder.join("create_autostart.ps1");

    fs::write(&script_path, script_content).expect("Failed to write PowerShell script");

    // Execute PowerShell script to create the shortcut
    Command::new("powershell")
        .args([
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_path.to_string_lossy(),
        ])
        .output()
        .expect("Failed to execute PowerShell script");

    println!("Clipboard Manager set to auto-start on Windows boot.");
}

#[cfg(target_os = "macos")]
pub fn setup_autostart_macos(binary_path: &Path) {
    let script_content = format!(
        "osascript -e 'tell application \"System Events\" to make login item at end with properties {{name:\"Clipboard Manager\", path:\"{}\", hidden:false}}'",
        binary_path.display()
    );

    let output = Command::new("sh")
        .arg("-c")
        .arg(script_content)
        .output()
        .expect("Failed to execute AppleScript");

    if output.status.success() {
        println!("Clipboard Manager added to macOS startup.");
    } else {
        println!("Failed to add Clipboard Manager to macOS startup.");
    }
}
