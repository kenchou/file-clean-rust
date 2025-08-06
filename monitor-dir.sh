#!/bin/bash
if [[ -z $1 ]]; then
    echo "Usage:" $(basename $0) "[PATH...]"
    exit 1
fi

BIN_PATH=$(dirname $0)

# Detect OS and set appropriate file monitoring tool
detect_file_watcher() {
    if command -v inotifywait >/dev/null 2>&1; then
        echo "inotifywait"
    elif command -v fswatch >/dev/null 2>&1; then
        echo "fswatch"
    else
        echo "error"
    fi
}

# Function to wait for directory to become stable
wait_for_directory_stable() {
    local target_dir="$1"
    local is_new_create="$2"  # Flag for newly created directory, true/false
    local max_wait=60  # Maximum wait time (seconds)
    local stable_time=3  # Stable time (seconds)
    local last_change=0
    local start_time=$(date +%s)

    echo "Waiting for directory to stabilize: $target_dir"

    local watcher=$(detect_file_watcher)

    if [ "$watcher" = "error" ]; then
        echo "Warning: No file monitoring tool found, using fixed delay"
        sleep 5
        # Check for newly created empty directory before proceeding
        if [ "$is_new_create" = true ] && [ -z "$(ls -A "$target_dir")" ]; then
            echo "Newly created and stable empty directory, no cleanup needed."
            return 1  # Return non-zero to indicate no processing needed
        fi
        return 0
    fi

    # Monitor directory changes using appropriate tool
    while true; do
        current_time=$(date +%s)
        elapsed=$((current_time - start_time))

        # If maximum wait time exceeded, proceed anyway
        if [ $elapsed -gt $max_wait ]; then
            echo "Wait timeout, proceeding with directory processing"
            break
        fi

        local has_change=false

        if [ "$watcher" = "inotifywait" ]; then
            # Linux/Unix systems use inotifywait
            if timeout $stable_time inotifywait -qq -r -e create,moved_to,modify "$target_dir" 2>/dev/null; then
                has_change=true
            fi
        elif [ "$watcher" = "fswatch" ]; then
            # macOS systems use fswatch
            if timeout $stable_time fswatch -1 -r "$target_dir" >/dev/null 2>&1; then
                has_change=true
            fi
        fi

        if [ "$has_change" = true ]; then
            # New file activity detected, reset timer
            last_change=$(date +%s)
            echo "File changes detected, continuing to wait..."
        else
            # Timeout occurred, no file changes in stable_time seconds
            if [ $last_change -gt 0 ]; then
                stable_duration=$((current_time - last_change))
                if [ $stable_duration -ge $stable_time ]; then
                    # Skip cleanup only for newly created empty directories
                    if [ "$is_new_create" = true ] && [ -z "$(ls -A "$target_dir")" ]; then
                        echo "Newly created and stable empty directory, no cleanup needed."
                        return 1  # Return non-zero to indicate no processing needed
                    fi
                    echo "Directory stable for ${stable_time} seconds, starting processing"
                    break
                fi
            else
                # No changes detected on first check, directory already stable
                if [ "$is_new_create" = true ] && [ -z "$(ls -A "$target_dir")" ]; then
                    echo "Newly created and stable empty directory, no cleanup needed."
                    return 1  # Return non-zero to indicate no processing needed
                fi
                echo "Directory is stable, starting processing"
                break
            fi
        fi

        sleep 0.5
    done

    return 0  # Return zero to indicate processing should continue
}

# Detect and start appropriate file monitoring
watcher=$(detect_file_watcher)

if [ "$watcher" = "error" ]; then
    echo "Error: Need to install inotifywait (Linux) or fswatch (macOS)"
    echo "macOS install command: brew install fswatch"
    echo "Linux install command: apt-get install inotify-tools (Ubuntu/Debian) or yum install inotify-tools (RHEL/CentOS)"
    exit 1
fi

echo "Using file monitoring tool: $watcher"

if [ "$watcher" = "inotifywait" ]; then
    # Linux/Unix systems use inotifywait
    # Monitor original trigger events, don't monitor create to prevent manually mkdir directories from being deleted
    inotifywait --exclude '(.tmp)' -r -m --format '%w%f %e' -e create,moved_to,modify "$@" | while IFS= read -r line; do
        echo "Raw event: $line"
        # Check if it's a directory event (create or move)
        if [[ "$line" == *"CREATE,ISDIR"* ]] || [[ "$line" == *"MOVED_TO,ISDIR"* ]]; then
            event_part="${line##* }"
            path_part="${line% *}"
            if [[ "$line" == *"CREATE,ISDIR"* ]]; then
                echo "Directory creation event detected: $path_part"
                echo "Event type: $event_part"
                # Newly created directory, pass true
                if wait_for_directory_stable "$path_part" true; then
                    # Process directory only if function returns 0 (should process)
                    echo "Starting directory processing: $path_part"
                    "${BIN_PATH}/file-clean-rust" --prune "$path_part"
                fi
            else
                echo "Directory move event detected: $path_part"
                echo "Event type: $event_part"
                # Moved/renamed directory, pass false
                if wait_for_directory_stable "$path_part" false; then
                    # Process directory only if function returns 0 (should process)
                    echo "Starting directory processing: $path_part"
                    "${BIN_PATH}/file-clean-rust" --prune "$path_part"
                fi
            fi
        fi
    done
elif [ "$watcher" = "fswatch" ]; then
    # macOS systems use fswatch
    fswatch -r "$@" | while IFS= read -r changed_path; do
        echo "Change detected: $changed_path"

        # Check if it's a directory
        if [ -d "$changed_path" ]; then
            echo "Directory event detected: $changed_path"

            # Wait for directory to stabilize before processing
            if wait_for_directory_stable "$changed_path" false; then
                # Process directory only if function returns 0 (should process)
                echo "Starting directory processing: $changed_path"
                "${BIN_PATH}/file-clean-rust" --prune "$changed_path"
            fi
        fi
    done
fi
