use super::*;

#[test]
fn repository_change_keeps_the_review_visible_and_marks_it_out_of_date() {
    let initial = snapshot("new");
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            vec![ReviewStop {
                title: "Inspect behavior".to_owned(),
                category: AttentionCategory::Behavior,
                reason: "The behavior changes here.".to_owned(),
                target_id: request.first_target_id().to_owned(),
            }],
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request,
        result,
        stale: false,
    });
    review.open_selected_stop();
    let selected_target = review.selected_target().map(|target| target.id.clone());

    review.repository_changed(snapshot("newer"));

    assert!(review.stale());
    assert!(review.ready().is_none());
    assert_eq!(
        review.selected_target().map(|target| target.id.clone()),
        selected_target
    );
    let text = render_text(&review);
    for expected in [
        "Out of date",
        "Summary",
        "Review path",
        "Step 1/1 · one change",
        "[ Regenerate (Enter) ]",
    ] {
        assert!(text.contains(expected), "missing {expected:?}");
    }
    assert!(!text.contains("Your changes changed after this review"));
}

#[test]
fn stale_review_remains_navigable_but_cannot_stage() {
    let initial = two_file_snapshot();
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let ids = request.target_ids();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            ids.iter()
                .enumerate()
                .map(|(index, id)| ReviewStop {
                    title: format!("Step {index}"),
                    category: AttentionCategory::Behavior,
                    reason: "Review this change.".to_owned(),
                    target_id: id.clone(),
                })
                .collect(),
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request,
        result,
        stale: false,
    });
    review.open_selected_stop();
    review.repository_changed(snapshot("newer"));

    assert!(matches!(
        review.handle_event(&key('n'), Rect::new(0, 0, 100, 30), PaneSplit::default()),
        Some(ReviewEvent::Redraw)
    ));
    assert_eq!(review.selected_stop, 1);
    assert_eq!(
        review.selected_target().map(|target| target.id.as_str()),
        Some(ids[1].as_str())
    );
    assert!(
        review
            .handle_event(&key(' '), Rect::new(0, 0, 100, 30), PaneSplit::default())
            .is_none()
    );
}

#[test]
fn only_the_visible_regenerate_button_refreshes_a_stale_review() {
    let initial = snapshot("new");
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            vec![ReviewStop {
                title: "Inspect behavior".to_owned(),
                category: AttentionCategory::Behavior,
                reason: "The behavior changes here.".to_owned(),
                target_id: request.first_target_id().to_owned(),
            }],
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request,
        result,
        stale: false,
    });
    review.open_selected_stop();
    review.repository_changed(snapshot("newer"));
    let (_, hits) = render(&review);
    let button = hits.generate_area;
    review.generate_area = button;

    assert_eq!(
        button.width,
        u16::try_from("[ Regenerate (Enter) ]".len()).unwrap()
    );
    assert!(
        review
            .handle_event(
                &left_click(button.right(), button.y),
                Rect::new(0, 0, 100, 30),
                PaneSplit::default(),
            )
            .is_none()
    );
    assert!(matches!(
        review.handle_event(
            &left_click(button.x, button.y),
            Rect::new(0, 0, 100, 30),
            PaneSplit::default(),
        ),
        Some(ReviewEvent::Generate(_))
    ));
}

#[test]
fn refreshing_keeps_the_stale_review_until_new_results_arrive() {
    let initial = snapshot("new");
    let old_request = ReviewRequest::from_snapshot(&initial).unwrap();
    let result = old_request
        .validate_review(
            vec!["Old overview stays readable.".to_owned()],
            vec![ReviewStop {
                title: "Old review step".to_owned(),
                category: AttentionCategory::Behavior,
                reason: "This remains useful context.".to_owned(),
                target_id: old_request.first_target_id().to_owned(),
            }],
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request: old_request,
        result,
        stale: false,
    });
    review.open_selected_stop();
    review.repository_changed(snapshot("newer"));
    let Some(ReviewEvent::Generate(new_request)) =
        review.handle_event(&enter(), Rect::new(0, 0, 100, 30), PaneSplit::default())
    else {
        panic!("regeneration request");
    };

    let id = ApplicationCommandId(7);
    review.generation_queued(id, new_request);
    let text = render_text(&review);
    assert!(text.contains("Old overview stays readable"));
    assert!(!text.contains("Building your review"));
    assert!(text.contains("Preparing changes and starting Codex"));
    assert!(text.contains("Out of date"));

    assert!(review.accept(ReviewCodexTaskResult {
        id,
        outcome: ReviewCodexOutcome::Failed("Codex stopped unexpectedly.".to_owned()),
    }));
    let text = render_text(&review);
    assert!(text.contains("Old overview stays readable"));
    assert!(text.contains("Codex stopped unexpectedly"));
    assert!(text.contains("Regenerate"));
}
