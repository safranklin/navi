# Navi Demo: Startup & Model Switching
# Usage: vhs demos/start.tape

Output docs/demo-start.gif

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms
Set Theme "Catppuccin Mocha"

# Build before recording so the GIF doesn't show compilation
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

# Session manager auto-opens on startup
Wait+Screen@30s /Sessions/
Sleep 1.5s

# Create a new session
Type "n"
Wait+Screen@5s /Input/
Sleep 1s

# Immediately open model picker and switch to GLM-5
Ctrl+P
Wait+Screen@5s /Models/
Sleep 1s

# Search for GLM
Type "glm"
Sleep 1s

# Select GLM-5
Enter
Sleep 1.5s

# Send a quick message
Type "Hello World!"
Sleep 500ms
Enter

# Wait for response
# Wait for spinner to stop (idle title bar: "navi │" without braille char)
Sleep 2s
Wait+Screen@90s /navi │/
Sleep 3s

# Exit
Ctrl+C
Sleep 500ms
