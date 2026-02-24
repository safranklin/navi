# Navi Demo
# Usage: vhs demo.tape

Output docs/demo.gif

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 600
Set Padding 20
Set TypingSpeed 75ms
Set Theme "Catppuccin Mocha"

# Start navi (builds on first run if needed)
Type "cargo run --release"
Enter

# Wait for the app to fully start
Wait+Screen@120s /Welcome to Navi/
Sleep 1s

# --- Scene 1: Tool use with chained arithmetic ---
# This triggers add → multiply → divide tool calls
Type "What is ((3+3) * 5) / 2?"
Sleep 500ms
Enter

# Wait for "Response complete." in the title bar
Wait+Screen@30s /Response complete/
Sleep 3s

# --- Scene 2: Cycle reasoning to High ---
# Auto → Low → Medium → High (3 presses)
Ctrl+R
Sleep 500ms
Ctrl+R
Sleep 500ms
Ctrl+R
Sleep 1s

# --- Scene 3: Reasoning question (with High effort) ---
Type "Could you have computed that in fewer tool calls? What's the minimum?"
Sleep 500ms
Enter

# Wait for "Response complete." in the title bar
Wait+Screen@60s /Response complete/
Sleep 3s

# Exit
Ctrl+C
Sleep 1s
