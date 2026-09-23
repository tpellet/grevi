# Adjudication

Both passes label every case, and `gold.jsonl` carries both labels for all 103 cases with
`agreed` saying whether they matched. The two passes agree on 95 of them; the other 8 are settled
below by reading the case input again, and each one's ground is repeated in the case's
`adjudication` field.

The 95 agreements need no adjudication: the gold is the label both passes gave, and both labels
stay in `gold.jsonl` so a reader can see what the agreement rested on. Ten of those 95 carry a
`medium` confidence from at least one pass, with the doubt written in that pass's `note`.

## The eight disagreements

| Case | Pass A | Pass B | Gold | Ground |
|:---|:---|:---|:---|:---|
| `fill-zox-08` | `man/man1` | `ambiguous` | `{"any_of": ["man/man1", "man"]}` | `man` holds nothing but `man1`, and `man1` holds the six pages. Either answer puts the caller in front of the manual pages, so both are right and neither is an abstention. |
| `pick-del-02` | `55` | `ambiguous` | `ambiguous` | Line 55 is `test_parse_hunk_header_with_no_hunk_lengths` and line 56 is `test_parse_hunk_header_with_omitted_hunk_lengths`. The two names describe the same condition, and the phrase "carries no hunk lengths" separates neither. Two records fit equally, which is what `ambiguous` means. |
| `why-c-link-undefined` | `[1, 4]` | `[1, 3]` | `[1, 4]` | Lines 1 to 4 are one report of one fault: the undefined `_checksum`, where it is referenced from, and the linker's summary of the same miss. Line 5 is the driver reporting that the linker exited, which is a consequence. |
| `why-py-json-decode` | `[18, 21]` | `[21, 21]` | `[21, 21]` | The frames on lines 7 to 20 are inside the standard library's `json` module, and pointing a reader at them names no fault. Line 21 is the only line that says what is wrong with the file. The other Python cases keep the failing frame because that frame is the caller's own code; here it is not. |
| `why-sh-set-e` | `[10, 11]` | `[11, 11]` | `[11, 11]` | Line 10 is the loop's ordinary progress output, identical in form to lines 8 and 9, which succeeded. Line 11 is the only line that reports a failure. |
| `filter-jst-05` | `unsure` | `keep` | `unsure` | The record is the subject `Escape list heading default newline in man page (#3692)`. Whether the man page is a file in the tree or output the code generates decides whether the change is documentation only, and the record does not say. Neither `keep` nor `drop` follows from what the annotator can see. |
| `filter-jst-06` | `unsure` | `drop` | `drop` | The record is `Release 1.58.0 (#3686)`. A release commit changes a version number as well as a changelog, and a version is not documentation, so the statement is false of this record. That much follows from the record alone. |
| `route-med-03` | `ambiguous` | `ffprobe` | `{"any_of": ["ffprobe", "mediainfo"]}` | `ffprobe` names codecs and bitrate outright and `mediainfo` reports the same facts in a readable form. Both serve the intent, so either name is a right answer and an abstention is a miss. |

## Method

Pass A labels every case from the case's input. Pass B labels every case again from the same
input, with pass A's file closed, and writes its own reason where a second reading is defensible.
Both passes are runs of one Claude Opus agent, the agent that built the set, working from
`evals/validation/annotator-instructions.md` and the answer space each verb's row in
`../README.md` states. The two passes are therefore not two annotators: they are two readings by
one reader, and a bias that survives a second reading survives into the gold. The disagreement
rate below, 8 of 103, is what that method produced, and it is the number to compare against the
12 of 153 that two separate models produced on `evals/validation/`.

No backend and no jevify run takes part: no case is labelled from what a run answered, and the
gold is fixed before the first request.
