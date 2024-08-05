#!/bin/bash
if [[ -z $1 ]]; then
    echo "Usage:" $(basename $0) "[PATH...]"
    exit 1
fi

BIN_PATH=$(dirname $0)
inotifywait --exclude '(.tmp)' -r -me moved_to "$@" | while read dir action file; do
    echo "The file '$file' appeared in directory '$dir' via '$action'"
    # do something with the file
    ${BIN_PATH}/file-clean-rust --prune "${dir}/${file}"
done
