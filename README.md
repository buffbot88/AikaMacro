# osk-typer

A small Windows command-line tool that focuses Notepad, pastes supplied Unicode text, then automatically clicks configured buttons in Windows On-Screen Keyboard (OSK). It supports up to eight programmable keys, Ctrl/Shift modifiers, and three per-key timers: press delay, hold delay, and delay before the next key.

## Requirements

- Windows
- Rust toolchain
- `osk.exe` and Notepad open before running

## Usage

On first run without an existing configuration, the tool interactively asks for up to eight OSK key bindings. Each binding has this format:

```text
key|ctrl|shift|press_ms|hold_ms|between_ms
```

For example, `A|true|false|50|100|75` clicks `A` with Ctrl enabled, waits 50 ms after pressing it, holds for 100 ms, then waits 75 ms before the next configured key. The generated `osk-typer.conf` is stored beside the executable and can be edited directly.

Run the configured sequence with:

```powershell
cargo run -- "Hello from the OSK"
```

The optional second argument is the delay after clicking Enter, in milliseconds:

```powershell
cargo run -- "Line one`nLine two" 100
```

The tool finds visible windows by title/class, restores and focuses Notepad, uses the clipboard for reliable Unicode text entry, and clicks the OSK Enter key with the mouse. Windows may show a consent prompt or block synthetic input if the target runs at a higher integrity level; run both applications at the same privilege level.

## Build

```powershell
cargo build --release
.target\release\osk-typer.exe "Hello"
```

Use this only for applications you control and expect to receive automated input.
