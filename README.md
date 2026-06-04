# cmdirstat

A fast, interactive terminal disk usage analyzer for Windows and Linux, written in Rust.

It scans directories in parallel, visualizes storage distribution with progress bars, and allows you to search and delete files or folders directly in the terminal interface.

## Usage

### Pre-compiled Binary (Windows)
If you downloaded the binary from the releases page, run it in terminal/PowerShell:
```powershell
.\cmdirstat.exe
```

### Building from Source
Make sure you have Rust installed.

#### Linux / WSL
Install build tools:
```bash
sudo apt update && sudo apt install -y build-essential
```
Build and run:
```bash
cargo build --release
./target/release/cmdirstat
```

#### Windows
Build and run (PowerShell / CMD):
```powershell
cargo build --release
.\target\release\cmdirstat.exe
```

## Controls

- `↑` / `k` — Navigate up
- `↓` / `j` — Navigate down
- `Enter` / `→` / `l` — Enter folder or select drive
- `Backspace` / `←` / `h` — Go back
- `d` / `Delete` — Delete selected file or folder (requires confirmation)
- `/` — Search / filter
- `Tab` / `s` — Cycle sort column
- `Space` / `r` — Toggle sort direction
- `?` / `F1` — Help menu
- `q` / `Esc` — Quit / close modal

## License

MIT
