# Navi Demo: Modes & Reasoning Effort
# Usage: vhs demos/modes.tape
#
# Demonstrates effort cycling and cursor mode for message navigation.

Output docs/demo-modes.gif

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms
Set Theme "Catppuccin Mocha"

# Build before recording
Hide
Type "cargo build --release"
Enter
Wait+Screen@180s /Compiling|Finished|>/
Type "clear"
Enter
Sleep 1s
Show

# Start navi (already compiled)
Type "cargo run --release"
Enter

# Dismiss session manager
Wait+Screen@30s /Sessions/
Sleep 500ms
Escape
Wait+Screen@5s /Input/
Sleep 500ms

# Send a quick message first so we have content to navigate
Type "Explain closures in Rust in two sentences"
Sleep 500ms
Enter
# Wait for spinner to stop
Sleep 2s
Wait+Screen@90s /navi │/
Sleep 2s

# Cycle reasoning effort: Auto → Low → Medium → High
Ctrl+R
Sleep 500ms
Ctrl+R
Sleep 500ms
Ctrl+R
Sleep 1s

# Ask a reasoning question with High effort
Type "What's the difference between Fn, FnMut, and FnOnce?"
Sleep 500ms
Enter
# Wait for spinner to stop
Sleep 2s
Wait+Screen@90s /navi │/
Sleep 3s

# Enter cursor mode (Esc) — input box dims
Escape
Sleep 1s

# Navigate up through messages
Up
Sleep 700ms
Up
Sleep 700ms
Up
Sleep 700ms

# Return to input mode by pressing Enter
Enter
Sleep 1s

# Exit
Ctrl+C
Sleep 500ms
