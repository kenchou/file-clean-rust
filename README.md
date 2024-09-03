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

## Configuration

The default configuration file `.cleanup-patterns.yml` is searched for starting from the specified target path,  
moving upwards step by step until the root directory is reached.  
If it is not found, it will then be looked for in the user's home directory.

```yaml
remove: |-
  # Any line that starts with '#' is treated as a comment.
  # Match the filename exactly.
  example_filename.ext
  # '*' and '?' are wildcards.
  wildcard*
  # For more complex matching, use regular expressions.
  # Notice: regex must start with "/".
  /regex_pattern1
  /regex_pattern2
remove_hash:
  "filename_or_wildcard":
    - md5hash
cleanup: |-
  # The filename cleaning rules only support regular expressions, 
  # so there is no need to start with '/'. 
  # The matched strings will be replaced with an empty string.
  regex_pattern1
  regex_pattern2
```

## Related projects
- [aria2](https://github.com/aria2/aria2)
- [aria2rpc-oversee](https://github.com/kenchou/aria2rpc-oversee)
