mod common;
use common::{FakeJev, option_containing};
use grevi::tournament::{Prompts, rank};

fn prompts() -> Prompts {
    Prompts {
        choose: "Which item?".into(),
        none: "none".into(),
        any: "Any?".into(),
    }
}

#[tokio::test]
async fn finds_needle_across_windows() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "NEEDLE"),
        noul: |_, s| {
            if s.to_string().contains("NEEDLE") {
                0.95
            } else {
                0.05
            }
        },
    })
    .await;
    let cfg = common::config(&server);
    let client = grevi::jev::client::Client::new(&cfg).unwrap();
    let mut items: Vec<String> = (0..1000).map(|i| format!("line {i}")).collect();
    items[777] = "the NEEDLE is here".into();
    let r = rank(&client, "find the needle", &items, &prompts(), None)
        .await
        .unwrap();
    assert_eq!(r.candidates[0].index, 777);
    assert!(r.any > 0.9);
    // 5 windows + 1 finals round
    assert_eq!(cfg.meta().requests, 6);
}

#[tokio::test]
async fn long_lines_are_clipped_so_a_window_stays_under_budget() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| {
            // Every item must arrive clipped: 200 items share 60k chars, so ≤ 300 chars + "…" each.
            let items = s["items"].as_array().unwrap();
            assert_eq!(items.len(), 200);
            assert!(
                items
                    .iter()
                    .all(|i| i.as_str().unwrap().chars().count() < 320),
                "item not clipped"
            );
            option_containing(s, o, "NEEDLE")
        },
        noul: |_, _| 0.9,
    })
    .await;
    let cfg = common::config(&server);
    let client = grevi::jev::client::Client::new(&cfg).unwrap();
    let mut items: Vec<String> = (0..200)
        .map(|i| format!("{i} {}", "x".repeat(5_000)))
        .collect();
    items[3] = format!("NEEDLE {}", "y".repeat(5_000));
    let r = rank(&client, "find the needle", &items, &prompts(), None)
        .await
        .unwrap();
    assert_eq!(r.candidates[0].index, 3);
}
