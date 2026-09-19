mod common;
use common::{FakeJev, option_containing};

#[tokio::test(flavor = "multi_thread")]
async fn prints_matching_line_raw() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::hunch(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["pick", "the bill"])
            .write_stdin("notes.txt\ninvoice-march.pdf\nphoto.jpg\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "invoice-march.pdf\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn abstains_with_exit_3() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.05,
    })
    .await;
    let mut c = common::hunch(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["pick", "a spaceship"])
            .write_stdin("a\nb\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn top_n_prints_only_lines_that_beat_none() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::hunch(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["pick", "-n", "3", "the bill"])
            .write_stdin("notes.txt\ninvoice-march.pdf\nphoto.jpg\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "invoice-march.pdf\n"
    );
}

// Blank lines take no window slot and a repeated line is sent once, but `line` still counts
// original stdin lines: this test fails if the `kept[c.index]` remap is transcribed as the old
// `c.index + 1`, and if the duplicate `notes.txt` reaches the model as a third item.
#[tokio::test(flavor = "multi_thread")]
async fn blank_and_duplicate_lines_are_skipped_but_line_numbers_are_original() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| {
            assert_eq!(s["items"].as_array().unwrap().len(), 2, "duplicate sent");
            option_containing(s, o, "invoice")
        },
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::hunch(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "pick", "--index", "the bill"])
            .write_stdin("notes.txt\n\ninvoice-march.pdf\nnotes.txt\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["matches"][0]["line"], 3, "{v}");
    assert_eq!(v["data"]["matches"][0]["text"], "invoice-march.pdf");
}

#[tokio::test(flavor = "multi_thread")]
async fn abstains_when_none_wins_the_choice() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::hunch(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["pick", "a spaceship"])
            .write_stdin("a\nb\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(3));
}
