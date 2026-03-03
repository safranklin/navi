# Navi Demo: Agentic Tool Use
# Usage: vhs demos/tools.tape
#
# Demonstrates chained arithmetic tool calls with collapsible result blocks.

Output docs/demo-tools.gif

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

# Trigger chained tool calls: add → multiply → divide
Type "What is ((3+3) * 5) / 2?"
Sleep 500ms
Enter

# Wait for the last tool in the chain (add → multiply → divide),
# then wait for the final text response to finish streaming.
Sleep 2s
Wait+Screen@60s /divide/
Wait+Screen@60s /navi │/
Sleep 3s

# Exit
Ctrl+C
Sleep 500ms
