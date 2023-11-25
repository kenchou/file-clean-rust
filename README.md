# file-clean-rust

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
