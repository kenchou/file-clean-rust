#!/bin/bash
if [[ -z $1 ]]; then
    echo "Usage:" $(basename $0) "[PATH...]"
    exit 1
fi

BIN_PATH=$(dirname $0)

# 检测操作系统并设置相应的文件监控工具
detect_file_watcher() {
    if command -v inotifywait >/dev/null 2>&1; then
        echo "inotifywait"
    elif command -v fswatch >/dev/null 2>&1; then
        echo "fswatch"
    else
        echo "error"
    fi
}

# 等待目录稳定的函数
wait_for_directory_stable() {
    local target_dir="$1"
    local max_wait=60  # 最大等待时间（秒）
    local stable_time=3  # 稳定时间（秒）
    local last_change=0
    local start_time=$(date +%s)

    echo "等待目录稳定: $target_dir"

    local watcher=$(detect_file_watcher)

    if [ "$watcher" = "error" ]; then
        echo "警告: 未找到文件监控工具，使用固定延迟"
        sleep 5
        return
    fi

    # 使用相应的工具监控目录变化
    while true; do
        current_time=$(date +%s)
        elapsed=$((current_time - start_time))

        # 如果超过最大等待时间，直接返回
        if [ $elapsed -gt $max_wait ]; then
            echo "等待超时，继续处理目录"
            break
        fi

        local has_change=false

        if [ "$watcher" = "inotifywait" ]; then
            # Linux/Unix 系统使用 inotifywait
            if timeout $stable_time inotifywait -qq -r -e create,moved_to,modify "$target_dir" 2>/dev/null; then
                has_change=true
            fi
        elif [ "$watcher" = "fswatch" ]; then
            # macOS 系统使用 fswatch
            if timeout $stable_time fswatch -1 -r "$target_dir" >/dev/null 2>&1; then
                has_change=true
            fi
        fi

        if [ "$has_change" = true ]; then
            # 有新的文件活动，重置计时器
            last_change=$(date +%s)
            echo "检测到文件变化，继续等待..."
        else
            # 超时了，说明在 stable_time 秒内没有文件变化
            if [ $last_change -gt 0 ]; then
                stable_duration=$((current_time - last_change))
                if [ $stable_duration -ge $stable_time ]; then
                    echo "目录已稳定 ${stable_time} 秒，开始处理"
                    break
                fi
            else
                # 首次检查就没有变化，说明目录已经稳定
                echo "目录已稳定，开始处理"
                break
            fi
        fi

        sleep 0.5
    done
}

# 检测并启动相应的文件监控
watcher=$(detect_file_watcher)

if [ "$watcher" = "error" ]; then
    echo "错误: 需要安装 inotifywait (Linux) 或 fswatch (macOS)"
    echo "macOS 安装命令: brew install fswatch"
    echo "Linux 安装命令: apt-get install inotify-tools (Ubuntu/Debian) 或 yum install inotify-tools (RHEL/CentOS)"
    exit 1
fi

echo "使用文件监控工具: $watcher"

if [ "$watcher" = "inotifywait" ]; then
    # Linux/Unix 系统使用 inotifywait
    inotifywait --exclude '(.tmp)' -r -m --format '%w %f %e' -e moved_to "$@" | while IFS= read -r line; do
        echo "原始事件: $line"
        # 检查是否包含 ISDIR
        if [[ "$line" == *"ISDIR"* ]]; then
            read dir file action <<< "$line"
            target_path="${dir}/${file}"
            echo "The directory '$file' appeared in directory '$dir' via '$action'"

            # 等待目录稳定后再处理
            wait_for_directory_stable "$target_path"

            # 处理目录
            echo "开始处理目录: $target_path"
            ${BIN_PATH}/file-clean-rust --prune "$target_path"
        fi
    done
elif [ "$watcher" = "fswatch" ]; then
    # macOS 系统使用 fswatch
    fswatch -r "$@" | while read -r changed_path; do
        echo "检测到变化: $changed_path"

        # 检查是否是新创建的目录
        if [ -d "$changed_path" ]; then
            # 检查目录是否是新移动过来的（简单的启发式检查）
            if [ -n "$(find "$changed_path" -mindepth 1 -maxdepth 1 2>/dev/null)" ]; then
                echo "检测到新目录: $changed_path"

                # 等待目录稳定后再处理
                wait_for_directory_stable "$changed_path"

                # 处理目录
                echo "开始处理目录: $changed_path"
                ${BIN_PATH}/file-clean-rust --prune "$changed_path"
            fi
        fi
    done
fi
