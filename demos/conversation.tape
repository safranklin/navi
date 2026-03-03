# Navi Demo: Markdown Conversation
# Usage: vhs demos/conversation.tape
#
# Prereq: Navi should be configured with a working model.

Output docs/demo-conversation.gif

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

# Ask a question that produces a code block
Type "Write a Python one-liner that checks if a number is prime"
Sleep 500ms
Enter

# Wait for spinner to stop (idle title bar: "navi │" without braille char)
Sleep 2s
Wait+Screen@90s /navi │/
Sleep 4s

# Exit
Ctrl+C
Sleep 500ms
