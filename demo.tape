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

# Pre-build off-camera
Hide
Type "cargo build --release 2>/dev/null"
Enter
Sleep 30s
Type "clear"
Enter
Sleep 500ms
Show

# Start navi
Type "cargo run --release"
Enter
Sleep 3s

# --- Scene 1: Hello World ---
Type "Hello World!"
Sleep 500ms
Enter

# Wait for the response to finish streaming
Sleep 8s

# --- Scene 2: Cycle reasoning to High ---
# Auto → Low → Medium → High (3 presses)
Ctrl+R
Sleep 500ms
Ctrl+R
Sleep 500ms
Ctrl+R
Sleep 1s

# --- Scene 3: Math question that triggers reasoning ---
Type "Prove that the integral of e^(-x^2) from -inf to inf equals sqrt(pi)."
Sleep 500ms
Enter

# Wait for reasoning + response to complete (Gaussian integral proof is heavy)
Sleep 30s

# Pause to admire the result
Sleep 3s

# Exit
Escape
Sleep 1s
