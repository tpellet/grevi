mod common;
use common::{FakeJev, option_containing};
use jevify::tournament::{Finalists, Prompts, rank};

fn prompts() -> Prompts {
    Prompts {
        choose: "Which item?".into(),
        none: "none".into(),
        any: "Any?".into(),
    }
}

fn needle_fake() -> FakeJev {
    FakeJev {
        choose: |_, s, o| option_containing(s, o, "NEEDLE"),
        noul: |_, _| 0.95,
    }
}

#[tokio::test]
async fn last_window_lower_scores_survive_and_close_rivals_meet() {
    use jevify::tournament::{Decision, decide};
    for ambiguous in [false, true] {
        let fake = needle_fake().with_probabilities(|_, state, options| {
            if options == ["yes", "no"] {
                return vec![0.9, 0.1];
            }
            let items = state["items"].as_array().unwrap();
            let finals = items.len() < 200;
            let has_needle = state.to_string().contains("NEEDLE");
            let close = state["request"] == "ambiguous";
            let winner = option_containing(state, options, "NEEDLE");
            let rival = option_containing(state, options, "RIVAL");
            options
                .iter()
                .map(|o| {
                    if finals && has_needle {
                        if *o == winner {
                            if close { 0.5 } else { 0.9 }
                        } else if *o == rival && close {
                            0.4
                        } else if o == "NONE" {
                            0.1
                        } else {
                            0.0
                        }
                    } else if has_needle {
                        if *o == winner {
                            0.4
                        } else if *o == rival {
                            0.35
                        } else if o == "NONE" {
                            0.25
                        } else {
                            0.0
                        }
                    } else if o == "L000" {
                        0.9
                    } else if o == "NONE" {
                        0.1
                    } else {
                        0.0
                    }
                })
                .collect()
        });
        let server = common::mock(fake).await;
        let cfg = common::config(&server);
        let client = jevify::jev::client::Client::new(&cfg).unwrap();
        let mut items: Vec<_> = (0..1800).map(|i| format!("item {i}")).collect();
        items[1798] = "NEEDLE".into();
        items[1799] = "RIVAL".into();
        let result = rank(
            &client,
            if ambiguous { "ambiguous" } else { "found" },
            &items,
            &prompts(),
            None,
            Finalists::Auto,
        )
        .await
        .unwrap();
        assert_eq!(result.candidates[0].index, 1798);
        if ambiguous {
            assert!(
                matches!(decide(&result, 0.5), Decision::Ambiguous(ref pair) if pair[1].index == 1799)
            );
        } else {
            assert!(matches!(decide(&result, 0.5), Decision::Found(_)));
        }
    }
}

fn classifier_config(server: &wiremock::MockServer) -> jevify::config::Config {
    jevify::config::Config {
        backend: jevify::config::Backend::Classifier,
        key: None,
        key_file: None,
        base_url: server.uri(),
        model: "jev-1.13.0".into(),
        threshold: 0.5,
        concurrency: 8,
        cache_dir: None,
        price_per_mtok: 0.042,
        stats: std::sync::Arc::new(jevify::jev::client::Stats::default()),
    }
}

fn states(requests: &[wiremock::Request]) -> Vec<serde_json::Value> {
    requests
        .iter()
        .filter(|r| r.method == "POST")
        .map(|r| {
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            if body.get("state").is_some() {
                body["state"].clone()
            } else {
                serde_json::from_str(body["items"][0].as_str().unwrap()).unwrap()
            }
        })
        .collect()
}

#[tokio::test]
async fn both_backends_capacity_and_request_boundaries() {
    for classifier in [false, true] {
        for count in if classifier {
            vec![99, 100, 250, 3267, 4000, 9801]
        } else {
            vec![200, 201, 250, 13200]
        } {
            let server = if classifier {
                common::mock_classifier(needle_fake()).await
            } else {
                common::mock(needle_fake()).await
            };
            let cfg = if classifier {
                classifier_config(&server)
            } else {
                common::config(&server)
            };
            let client = jevify::jev::client::Client::new(&cfg).unwrap();
            let w = cfg.backend.window();
            let mut items: Vec<_> = (0..count).map(|i| format!("item {i}")).collect();
            items[count - 1] = "NEEDLE".into();
            let mode = if count == w * (w / 3) {
                Finalists::ThreeOnly
            } else {
                Finalists::Auto
            };
            let result = rank(&client, "needle", &items, &prompts(), None, mode)
                .await
                .unwrap();
            assert_eq!(result.candidates[0].index, count - 1);
            let windows = count.div_ceil(w);
            let n = mode.per_window(count, w).unwrap();
            assert_eq!((result.windows, result.n), (windows, n));
            let captured = states(&server.received_requests().await.unwrap());
            assert_eq!(captured.len(), windows + usize::from(windows > 1));
            if windows > 1 {
                assert_eq!(
                    captured.last().unwrap()["items"].as_array().unwrap().len(),
                    (0..windows).map(|i| n.min(count - i * w)).sum::<usize>()
                );
            }
        }
        let server = if classifier {
            common::mock_classifier(needle_fake()).await
        } else {
            common::mock(needle_fake()).await
        };
        let cfg = if classifier {
            classifier_config(&server)
        } else {
            common::config(&server)
        };
        let client = jevify::jev::client::Client::new(&cfg).unwrap();
        let w = cfg.backend.window();
        for (count, mode) in [
            (w * w + 1, Finalists::Auto),
            (w * (w / 3) + 1, Finalists::ThreeOnly),
        ] {
            let error = rank(
                &client,
                "needle",
                &vec!["item".into(); count],
                &prompts(),
                None,
                mode,
            )
            .await
            .unwrap_err();
            assert!(matches!(
                error,
                jevify::exit::JevifyError::Kinded {
                    kind: "too_many",
                    ..
                }
            ));
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn shortlist_preserves_rank_then_window_order_and_evidence() {
    use jevify::tournament::shortlist;
    let server = common::mock(needle_fake().with_probabilities(|_, state, options| {
        if options == ["yes", "no"] {
            return vec![0.9, 0.1];
        }
        let first = state["items"][0].as_str().unwrap();
        let scores = if first.ends_with("item 0") {
            [0.6, 0.2, 0.1]
        } else {
            [0.4, 0.3, 0.2]
        };
        options
            .iter()
            .map(|o| match o.as_str() {
                "L000" => scores[0],
                "L001" => scores[1],
                "L002" => scores[2],
                "NONE" => 0.1,
                _ => 0.0,
            })
            .collect()
    }))
    .await;
    let cfg = common::config(&server);
    let client = jevify::jev::client::Client::new(&cfg).unwrap();
    let items: Vec<_> = (0..600).map(|i| format!("item {i}")).collect();
    let short = shortlist(&client, "order", &items, &prompts(), Finalists::Auto)
        .await
        .unwrap();
    assert_eq!(short.windows.len(), 3);
    assert!(short.windows.iter().all(|r| r.any == 0.9 && r.none == 0.1));
    let expected = vec![0, 200, 400, 1, 201, 401, 2, 202, 402];
    assert_eq!(
        short.finalists.iter().map(|c| c.index).collect::<Vec<_>>(),
        expected
    );
    rank(&client, "order", &items, &prompts(), None, Finalists::Auto)
        .await
        .unwrap();
    let captured = states(&server.received_requests().await.unwrap());
    let finals = captured.last().unwrap()["items"].as_array().unwrap();
    for (item, index) in finals.iter().zip(expected) {
        assert!(item.as_str().unwrap().ends_with(&format!("item {index}")));
    }
}

#[tokio::test]
async fn one_window_decisions_need_one_request() {
    use jevify::tournament::{Decision, decide, shortlist};
    for (text, low) in [("NEEDLE", false), ("NEEDLE", true), ("nothing", false)] {
        let server = common::mock(FakeJev {
            choose: needle_fake().choose,
            noul: if low { |_, _| 0.1 } else { |_, _| 0.9 },
        })
        .await;
        let cfg = common::config(&server);
        let client = jevify::jev::client::Client::new(&cfg).unwrap();
        let short = shortlist(
            &client,
            "needle",
            &[text.into()],
            &prompts(),
            Finalists::Auto,
        )
        .await
        .unwrap();
        let decision = decide(&short.windows[0], 0.5);
        assert_eq!(
            matches!(decision, Decision::Found(_)),
            text == "NEEDLE" && !low
        );
        assert_eq!(cfg.meta().requests, 1);
    }
}

#[tokio::test]
async fn planted_answer_survives_every_position_of_1800_items() {
    let server = common::mock(needle_fake()).await;
    let cfg = common::config(&server);
    let client = jevify::jev::client::Client::new(&cfg).unwrap();
    let mut items: Vec<_> = (0..1800).map(|i| format!("item {i}")).collect();
    for index in 0..items.len() {
        let original = std::mem::replace(&mut items[index], "NEEDLE".into());
        let result = rank(&client, "needle", &items, &prompts(), None, Finalists::Auto)
            .await
            .unwrap();
        assert_eq!(result.candidates[0].index, index);
        items[index] = original;
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
    let client = jevify::jev::client::Client::new(&cfg).unwrap();
    let mut items: Vec<String> = (0..1000).map(|i| format!("line {i}")).collect();
    items[777] = "the NEEDLE is here".into();
    let r = rank(
        &client,
        "find the needle",
        &items,
        &prompts(),
        None,
        Finalists::Auto,
    )
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
    let client = jevify::jev::client::Client::new(&cfg).unwrap();
    let mut items: Vec<String> = (0..200)
        .map(|i| format!("{i} {}", "x".repeat(5_000)))
        .collect();
    items[3] = format!("NEEDLE {}", "y".repeat(5_000));
    let r = rank(
        &client,
        "find the needle",
        &items,
        &prompts(),
        None,
        Finalists::Auto,
    )
    .await
    .unwrap();
    assert_eq!(r.candidates[0].index, 3);
}
