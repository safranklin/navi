# Navi Demo: Emacs-Style Editing
# Usage: vhs demos/editing.tape
#
# Demonstrates kill/yank, word deletion, and input history.

Output docs/demo-editing.gif

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

# Type a sentence
Type "the quick brown fox jumps over the lazy dog"
Sleep 1s

# Jump to start (Ctrl+A)
Ctrl+A
Sleep 700ms

# Kill to end of line (Ctrl+K) — cuts entire line
Ctrl+K
Sleep 1s

# Yank it back (Ctrl+Y) — text reappears
Ctrl+Y
Sleep 1.5s

# Delete words from end (Ctrl+W x3): "dog", "lazy", "the"
Ctrl+W
Sleep 500ms
Ctrl+W
Sleep 500ms
Ctrl+W
Sleep 1s

# Submit the remaining message
Enter
# Wait for spinner to stop
Sleep 2s
Wait+Screen@90s /navi │/
Sleep 2s

# Show input history — press Up to recall last message
Up
Sleep 1.5s

# Dismiss and exit
Ctrl+C
Sleep 500ms
