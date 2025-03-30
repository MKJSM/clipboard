// Desktop icon creation for different platforms

use dirs::home_dir;
use std::fs;
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "linux")]
pub fn setup_desktop_shortcut(binary_path: &Path) {
    let applications_dir = home_dir().unwrap().join(".local/share/applications");

    // Ensure the directory exists
    if !applications_dir.exists() {
        fs::create_dir_all(&applications_dir).expect("Failed to create applications directory");
    }

    let desktop_file = applications_dir.join("clipboard-manager.desktop");

    let desktop_content = format!(
        "[Desktop Entry]
        Type=Application
        Name=Clipboard Manager
        Exec={}
        Icon=edit-paste
        Terminal=false
        Categories=Utility;",
        binary_path.display()
    );

    fs::write(&desktop_file, desktop_content).expect("Failed to write desktop entry");

    // Make it executable
    Command::new("chmod")
        .args(["+x", &desktop_file.to_string_lossy()])
        .output()
        .expect("Failed to make desktop shortcut executable");

    println!("Desktop shortcut created at {:?}", desktop_file);
}

#[cfg(target_os = "windows")]
pub fn setup_desktop_shortcut(binary_path: &Path) {
    let desktop_dir = home_dir().unwrap().join("Desktop");

    let shortcut_path = desktop_dir.join("Clipboard Manager.lnk");

    let script_content = format!(
        "$WScriptShell = New-Object -ComObject WScript.Shell
        $Shortcut = $WScriptShell.CreateShortcut('{}')
        $Shortcut.TargetPath = '{}'
        $Shortcut.Save()",
        shortcut_path.display(),
        binary_path.display()
    );

    let script_path = desktop_dir.join("create_shortcut.ps1");

    fs::write(&script_path, script_content).expect("Failed to write PowerShell script");

    // Run PowerShell script to create shortcut
    Command::new("powershell")
        .args([
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_path.to_string_lossy(),
        ])
        .output()
        .expect("Failed to execute PowerShell script");

    println!("Desktop shortcut created at {:?}", shortcut_path);
}

#[cfg(target_os = "macos")]
pub fn setup_desktop_shortcut(binary_path: &Path) {
    let desktop_dir = home_dir().unwrap().join("Desktop");

    let shortcut_path = desktop_dir.join("Clipboard Manager.app");

    let script_content = format!(
        "tell application \"Finder\" to make alias file to POSIX file \"{}\" at POSIX file \"{}\"",
        binary_path.display(),
        desktop_dir.display()
    );

    let script_path = desktop_dir.join("create_alias.scpt");

    fs::write(&script_path, script_content).expect("Failed to write AppleScript");

    // Run AppleScript to create alias
    Command::new("osascript")
        .arg(script_path.to_string_lossy())
        .output()
        .expect("Failed to execute AppleScript");

    println!("Desktop shortcut created at {:?}", shortcut_path);
}
