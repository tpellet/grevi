# demo.tape — jevify v0 demo (hunch-task-13c-demo-jk1)
# Fixture data only: no real home directory, no key material shown.
# Run: TYPESAFE_API_KEY_FILE=/path/to/key vhs demo.tape
Output demo.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set Theme "Dracula"
Set TypingSpeed 40ms

Hide
Type `REPO=$(pwd); D=$(mktemp -d "${TMPDIR:-/tmp}/jevify-demoXXXX"); cp -R "$REPO/benchmarks/fixtures/demo"/* "$D/"; export PATH="$REPO/target/release:$PATH"; cd "$D"; export PS1='demo$ '; clear`
Enter
Sleep 500ms
Show

# Scene 1: why — the line that broke the build, buried under 300 warnings.
Type `cd buildfail && cargo build 2>&1 | jevify --verbose why`
Enter
Sleep 6s
Type `cd ..`
Enter
Sleep 500ms

# Scene 2: pick — the fixture file name that matches the request.
Type `ls fixtures/downloads | jevify --verbose pick "last month's electricity bill"`
Enter
Sleep 4s

# Scene 3: is — a predicate on text, answered by exit code.
Type `jevify --verbose is "asks for a refund" < mail.txt; echo $?`
Enter
Sleep 4s

# Scene 4: filter — keep the billing records, in input order.
Type `ls fixtures/downloads | jevify --verbose filter "a bill or invoice"`
Enter
Sleep 5s

# Scene 5: route — an honest abstention.
Type `jevify --verbose route "make a qr code"`
Enter
Sleep 4s

Type `echo done`
Enter
Sleep 1s
