#!/bin/bash

# ---
# This script runs `cargo run` with arguments, splitting stdout and stderr
# into two tmux panes: top for stdout, bottom for stderr.
#
# It saves persistent, timestamped logs to a `./log` directory.
# ---

# Forward all script arguments to `cargo run`
CARGO_ARGS="$@"

# 1. Check for tmux dependency
if ! command -v tmux &> /dev/null; then
    echo "Error: tmux not found. Please install it to use this script."
    exit 1
fi

# 3. Create unique, timestamped log file paths
TIMESTAMP=$(date +%Y-%m-%d_%H-%M-%S)
OUT_FILE="./log/${TIMESTAMP}.concise"
ERR_FILE="./log/${TIMESTAMP}.verbose"

touch "$OUT_FILE"
touch "$ERR_FILE"

# 4. Define a unique session name
# Using $$ (the script's PID) makes it unique
SESSION_NAME="split_run_$$"

echo "Starting split view (stdout top | stderr bottom)"
echo "Logging stdout to: $OUT_FILE"
echo "Logging stderr to: $ERR_FILE"
echo ""
echo "Starting cargo run..."

# 5. Start cargo run in the background, redirecting output to files
cargo run -p kumad $CARGO_ARGS > "$OUT_FILE" 2> "$ERR_FILE" &
CARGO_PID=$!

# Give cargo a moment to start and create some output
sleep 0.5

# 6. Create the tmux layout
# Create a new session with stdout viewer on top
if ! tmux new-session -d -s "$SESSION_NAME" "less -R +F --mouse \"${OUT_FILE}\""; then
    echo "Error: Failed to create tmux session"
    kill $CARGO_PID 2>/dev/null
    exit 1
fi

# Split vertically, 50% height, for stderr on bottom
if ! tmux split-window -v -p 50 -t "${SESSION_NAME}:0.0" "less -R +F --mouse \"${ERR_FILE}\""; then
    echo "Error: Failed to split window for stderr"
    tmux kill-session -t "$SESSION_NAME"
    kill $CARGO_PID 2>/dev/null
    exit 1
fi

# Set up a hook to kill cargo when the session ends
tmux set-hook -t "$SESSION_NAME" session-closed "run-shell 'kill $CARGO_PID 2>/dev/null || true'"

# Focus the top pane (stdout)
tmux select-pane -t "${SESSION_NAME}:0.0"

# 7. Attach to the new session
tmux attach-session -t "$SESSION_NAME"

# Clean up cargo process when tmux exits
kill $CARGO_PID 2>/dev/null || true
