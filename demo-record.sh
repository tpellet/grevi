#!/bin/bash
# demo-record.sh — scripted terminal session for demo.gif (hunch-task-13c-demo-jk1)
#
# Deviation from the bead: vhs's headless Chromium triggered a macOS keychain
# prompt on the operator's screen (unattended run, nobody to answer it), so
# the recording pipeline is asciinema + agg instead of vhs. demo.tape is kept
# for reference; this script is the actual source used to produce demo.gif.
#
# Usage: TYPESAFE_API_KEY_FILE=/path/to/key ./demo-record.sh
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
D="$(mktemp -d "${TMPDIR:-/tmp}/jevify-demoXXXX")"
cp -R "$REPO/benchmarks/fixtures/demo"/* "$D/"
export PATH="$REPO/target/release:$PATH"
export PS1='demo$ '
cd "$D"

prompt() {
    printf 'demo$ %s\n' "$1"
}

pause() {
    sleep "${1:-1}"
}

clear
pause 1

# Scene 1: why — the line that broke the build, buried under 300 warnings.
cd buildfail
prompt 'cargo build 2>&1 | jevify --verbose why'
cargo build 2>&1 | jevify --verbose why
cd ..
pause 2

# Scene 2: pick — the fixture file name that matches the request.
prompt 'ls fixtures/downloads | jevify --verbose pick "last month'"'"'s electricity bill"'
ls fixtures/downloads | jevify --verbose pick "last month's electricity bill"
pause 2

# Scene 3: is — a predicate on text, answered by exit code.
prompt 'jevify --verbose is "asks for a refund" < mail.txt; echo $?'
jevify --verbose is "asks for a refund" < mail.txt
echo $?
pause 2

# Scene 4: filter — keep the billing records, in input order.
prompt 'ls fixtures/downloads | jevify --verbose filter "a bill or invoice"'
ls fixtures/downloads | jevify --verbose filter "a bill or invoice"
pause 1

# Scene 5: route — an honest abstention.
prompt 'jevify --verbose route "make a qr code"'
jevify --verbose route "make a qr code"
pause 2

prompt 'echo done'
echo done
pause 1
