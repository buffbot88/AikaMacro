# osk-typer

A Windows Rust utility that automates buttons in the Windows On-Screen Keyboard (OSK). The current executable provides a configuration flow and a one-shot macro runner for up to eight OSK buttons.

## Current functionality

- Up to 8 programmable slots
- Ctrl and Shift modifier actions
- Press duration, release delay, and between-key interval timers
- Versioned configuration file
- Cancellable, ordered macro action engine
- OSK key discovery and UI Automation activation without moving or clicking the mouse
- Notepad text-entry compatibility from the earlier CLI flow

## Usage

Run without arguments to create or edit the configuration:

```powershell
cargo run
```

Run a configured sequence:

```powershell
cargo run -- "text to paste"
```

The GUI always creates a fresh Windows On-Screen Keyboard (`osk.exe`) session. It closes any existing visible OSK windows, waits for them to exit, launches a new OSK instance, and closes that exact instance when the GUI exits. Open Notepad before starting a macro. The configuration file is stored beside the executable as `osk-macro.conf`.

Each slot stores these values:

```text
slot.N.enabled
slot.N.key
slot.N.ctrl
slot.N.shift
slot.N.interval_ms
slot.N.press_duration_ms
slot.N.release_delay_ms
```

Run without arguments to open the compact Win32 desktop bar. The Windows GUI build does not open a console window. The app replaces any existing OSK window with a fresh instance and cleans up that instance on exit. The UI provides visual key capture, a slot editor for Ctrl/Shift and all three timers, Save/Load, and global hotkeys (`Ctrl+P` to start and `Ctrl+S` to stop). Click a key field and press a physical key to capture it; click an interval field to edit the complete slot configuration. Press `Escape` during capture to clear the slot.

The earlier one-shot CLI flow remains available for text entry and compatibility testing.

## Configuration

The versioned configuration file is stored beside the executable as `osk-macro.conf`. The desktop editor validates timer values as non-negative integers no larger than `86,400,000` milliseconds. Saving writes through a temporary file before replacing the configuration.

Diagnostics are written to `osk-macro.log` beside the executable. Logging is enabled by default and records startup, OSK replacement, window creation, hotkeys, macro execution, errors, and shutdown. To disable file logging, add this line to `osk-macro.conf` and restart the app:

```text
logging=false
```

Logging is lazy, so when disabled the app does not create or append to the log file.

## Development checks

```powershell
cargo fmt --all -- --check
cargo check
cargo test
cargo build --release
```

The executable uses an `asInvoker` Windows manifest: it runs at the same privilege level as the launcher. OSK itself may require elevation, so the app starts it through Windows' `runas` elevation broker. Approve the UAC prompt, then launch the release executable with `Run as administrator` too so both processes can interact at the same integrity level. Macro keys are activated through Windows UI Automation and do not move or click the mouse.

If startup fails, the application shows an error dialog and records the detailed cause in `osk-macro.log` unless logging is disabled.
