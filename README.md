# osk-typer

A Windows Rust utility that automates buttons in the Windows On-Screen Keyboard (OSK). The current executable provides a configuration flow and a one-shot macro runner for up to eight OSK buttons.

## Current functionality

- Up to 8 programmable slots
- Ctrl and Shift modifier actions
- Press duration, release delay, and between-key interval timers
- Versioned configuration file
- Cancellable, ordered macro action engine
- OSK button discovery and mouse-click automation
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

Before running, open both Notepad and Windows On-Screen Keyboard (`osk.exe`). The configuration file is stored beside the executable as `osk-macro.conf`.

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

The compact Win32 desktop bar, visual key capture, slot editor popup, and global hotkeys are planned next. The current implementation intentionally keeps the working CLI while the UI layer is built incrementally.

## Development checks

```powershell
cargo fmt --all -- --check
cargo check
cargo test
```

The tool is intended for applications you control. Windows can reject synthetic input when the target process runs at a higher integrity level; run both applications at the same privilege level.
