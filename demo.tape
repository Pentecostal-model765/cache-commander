# VHS Demo — Cache Commander (ccmd)
# https://github.com/charmbracelet/vhs

Output demo.gif

Set Shell "bash"
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set Padding 20
Set TypingSpeed 80ms

# Launch ccmd with vuln and version checks
Type "ccmd --vulncheck --versioncheck"
Sleep 300ms
Enter
Sleep 3s

# Show help first
Type "?"
Sleep 10s
Escape
Sleep 1s

# Expand .cache (first root)
Right
Sleep 1s

# Navigate down to uv (6th entry inside .cache)
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms

# Now on uv — expand it
Right
Sleep 1s
Down
Sleep 800ms
Right
Sleep 1s
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms

# Collapse uv and move to huggingface (next sibling)
Left
Sleep 800ms
Down
Sleep 800ms

# Expand huggingface
Right
Sleep 1s
Down
Sleep 800ms
Right
Sleep 1s
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms

# Cycle sort
Type "s"
Sleep 1s
Type "s"
Sleep 1s

# Filter to vulnerable only
Type "f"
Sleep 1.5s

# Filter to outdated only
Type "f"
Sleep 1.5s

# Clear filter
Type "f"
Sleep 1s

# Quit
Type "q"
Sleep 500ms
