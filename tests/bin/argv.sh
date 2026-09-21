#!/bin/sh
status=$1
shift
for arg do
    printf '%s\000' "$arg"
done
if IFS= read -r line || [ -n "$line" ]; then
    printf 'stdin:data\n'
else
    printf 'stdin:eof\n'
fi
exit "$status"
