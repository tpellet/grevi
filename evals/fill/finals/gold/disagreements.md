# Adjudicated disagreements

The two annotators gave the same label on 32 of the 33 cases. Two labels were `none`
(`finals-dir-04`: ripgrep has no Dockerfile; `finals-dir-08`: fzf's Windows installer is a root
file, not a listed directory), agreed by both.

| Case | Annotator A | Annotator B | Gold | Ground |
|:---|:---|:---|:---|:---|
| `finals-file-06` "what a search reads from: a file, stdin or a memory map" | `crates/core/haystack.rs` | `crates/core/search.rs` | `ambiguous` | The phrase names the thing a search reads from, which is what `haystack.rs` defines ("a haystack represents something we want to search", with `is_stdin`), and enumerates the ways it is read, which is what `search.rs` does (`search_reader` on stdin, a file, or a memory map). Nothing in the phrase separates the two, so both fit equally and the right answer is to abstain. |

Annotator B marked four other labels medium confidence and gave the reason; annotator A gave the
same label on each, so they stand: `finals-file-20` `src/output.rs` (spawns the pager and
rewrites its arguments; `pager.rs` only picks which pager), `finals-file-23`
`src/bin/bat/directories.rs` (the only file touching the cache directory), `finals-file-24`
`src/printer.rs` (`wrapping.rs` is only the `WrappingMode` enum; the wrapping at the terminal
width is in the printer).
