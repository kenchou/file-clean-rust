# file-clean-rust

Clean up (rename/delete) folders and files according to configured rules.

## Motivation

Resources downloaded through P2P networks usually contain a lot of junk files or padding files.  
Some clients (such as xunlei) have automatic cleaning features, but `aria2` lacks this functionality.  
Therefore, I wrote a tool to clean up directories and files.

## Usage

```text
Usage: file-clean-rust [OPTIONS] [path]

Arguments:
  [path]  target path to clean up

Options:
  -c, --config <FILE>        Sets a custom config file
  -d, --delete               Match filename deletion rule. [default]
  -D, --no-delete            Do not match filename deletion rule.
  -x, --hash                 Match hash deletion rule. [default]
  -X, --no-hash              Do not match hash deletion rule.
  -r, --rename               Match file renaming rule. [default]
  -R, --no-rename            Do not match file renaming rule.
  -t, --skip-tmp             Skip the .tmp directory. [default]
  -T, --no-skip-tmp          Do not skip the .tmp directory.
  -e, --remove-empty-dir     Delete empty directories. [default]
  -E, --no-remove-empty-dir  Do not delete empty directories.
      --prune                Perform the prune action.
  -v, --verbose...           Verbose mode.
  -h, --help                 Print help
  -V, --version              Print version
```

example:  
`file-clean-rust ~/Downloads` dry-run and see result  
`file-clean-rust ~/Downloads --prune` prune the target path and see result

## Directory Monitoring

The `monitor-dir.sh` script provides real-time monitoring of directories for newly moved folders.  
When a directory is moved into the monitored path, it automatically runs `file-clean-rust` to clean it up.

### Features

- **Cross-platform support**: Works on Linux (using `inotifywait`) and macOS (using `fswatch`)
- **Smart waiting mechanism**: Waits for directory to stabilize before processing to ensure all files are moved
- **Safe path handling**: Correctly handles filenames with spaces and special characters
- **Timeout protection**: Maximum wait time to prevent infinite waiting

### Prerequisites

**Linux/Unix systems:**

```bash
# Ubuntu/Debian
sudo apt-get install inotify-tools

# RHEL/CentOS/Fedora
sudo yum install inotify-tools
# or
sudo dnf install inotify-tools
```

**macOS:**

```bash
brew install fswatch
```

### Script Usage

```bash
# Monitor a single directory
./monitor-dir.sh /data/Downloads/TV/

# Monitor multiple directories
./monitor-dir.sh /data/Downloads/TV/ /data/Downloads/Movies/
```

### Script Configuration

The script uses the following default settings:

- **Maximum wait time**: 60 seconds
- **Stability check time**: 3 seconds (waits for 3 seconds of no file activity)

You can modify these values in the `wait_for_directory_stable()` function:

```bash
local max_wait=60     # Maximum wait time (seconds)
local stable_time=3   # Stability time (seconds)
```

### How it works

1. Monitors specified directories for `moved_to` events
2. When a directory is moved in, starts monitoring that directory for file changes
3. Waits until no file activity is detected for the stability period
4. Runs `file-clean-rust --prune` on the stabilized directory
5. Continues monitoring for new directory movements

### Example Output

```text
使用文件监控工具: fswatch
检测到目录移动事件: /data/Downloads/TV/MyShow.S01.2025/Episode.01.1080p.WEB-DL
事件类型: MOVED_TO,ISDIR
等待目录稳定: /data/Downloads/TV/MyShow.S01.2025/Episode.01.1080p.WEB-DL
检测到文件变化，继续等待...
检测到文件变化，继续等待...
目录已稳定 3 秒，开始处理
开始处理目录: /data/Downloads/TV/MyShow.S01.2025/Episode.01.1080p.WEB-DL
正在扫描文件...
[... file-clean-rust output ...]
```

## File Cleanup Configuration

The default configuration file `.cleanup-patterns.yml` is searched for starting from the specified target path,  
moving upwards step by step until the root directory is reached.  
If it is not found, it will then be looked for in the user's home directory.

```yaml
remove: |-
  # Any line that starts with '#' is treated as a comment. # 任何以井号 '#' 开头的行都做为注释
  # Match the filename exactly. # 匹配精确的文件名
  example_filename.ext
  # '*' and '?' are wildcards.  # 可以使用通配符
  wildcard*
  # For more complex matching, use regular expressions. # 更复杂的匹配规则可以使用正则表达式
  # Notice: regex must start with "/".                  # 注意：正则表达式必需以斜杠 '/' 开头. (区别于通配符规则)
  /regex_pattern1
  /regex_pattern2
remove_hash:
  # Sometimes, files may have the same content but different names. # 有时候一些文件具有相同的内容，但是会有不同的名字；
  # Or they may have very common names (e.g., 01.jpg).              # 或者具有很常用的名字（比如 01.jpg），
  # You can use hash matching to delete them.                       # 可以使用哈希匹配来删除。
  # To improve efficiency, this method first matches the file names # 为了提升效率，此方法先匹配文件名，
  # and then calculates the hash values.                            # 再计算哈希值。
  # Only if both match will the file be deleted.                    # 二者都匹配才会删除。
  # The file name rules are the same as the `remove` rules          # 文件名规则同 `remove` 规则，
  # and support wildcards and regular expressions.                  # 支持通配符和正则表达式。
  # Note: It is not recommended to use wildcards like *.jpg,        # 注：不建议使用 *.jpg 这样的通配符，
  # as this may result in too many files needing hash calculation.  #    可能导致需要计算 hash 的文件过多。
  "filename_or_wildcard":
    - md5hash1
    - md5hash2
  "/file1|file2":
    - md5hash1
    - md5hash2
cleanup: |-
  # The filename cleaning rules only support regular expressions, # 文件名清理（改名）只支持正则表达式 
  # so there is no need to start with '/'.                        # 所以不需要使用斜杠 '/' 开头 
  # The matched strings will be replaced with an empty string.    # 匹配的内容会被替换成空串
  regex_pattern1
  regex_pattern2
```

## Related projects

- [aria2](https://github.com/aria2/aria2)
- [aria2rpc-oversee](https://github.com/kenchou/aria2rpc-oversee)
