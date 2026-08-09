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
                primary_hunk_id: request.first_hunk_id().to_owned(),
            }],
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview { request, result });
    review.open_selected_stop();
    let active_hunk_id = review.active_hunk_id.clone();

    review.repository_changed(snapshot("newer"));

    assert!(review.stale());
    assert!(review.ready().is_none());
    assert_eq!(review.active_hunk_id, active_hunk_id);
    let text = render_text(&review);
    for expected in [
        "Review out of date",
        "Summary",
        "Review order",
        "Selected change 1 of 1",
        "[ Regenerate review (Enter) ]",
    ] {
        assert!(text.contains(expected), "missing {expected:?}");
    }
    assert!(!text.contains("Your changes changed after this review"));
}

#[test]
fn stale_review_remains_navigable_but_cannot_stage() {
    let initial = two_file_snapshot();
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let ids = request.hunk_ids();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            ids.iter()
                .enumerate()
                .map(|(index, id)| ReviewStop {
                    title: format!("Step {index}"),
                    category: AttentionCategory::Behavior,
                    reason: "Review this change.".to_owned(),
                    primary_hunk_id: id.clone(),
                })
                .collect(),
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview { request, result });
    review.open_selected_stop();
    review.repository_changed(snapshot("newer"));

    assert!(matches!(
        review.handle_event(&key('n'), Rect::new(0, 0, 100, 30), PaneSplit::default()),
        Some(ReviewEvent::Redraw)
    ));
    assert_eq!(review.selected_stop, 1);
    assert_eq!(review.active_hunk_id.as_deref(), Some(ids[1].as_str()));
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
                primary_hunk_id: request.first_hunk_id().to_owned(),
            }],
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview { request, result });
    review.open_selected_stop();
    review.repository_changed(snapshot("newer"));
    let (_, hits) = render(&review);
    let button = hits.generate_area;
    review.generate_area = button;

    assert_eq!(
        button.width,
        u16::try_from("[ Regenerate review (Enter) ]".len()).unwrap()
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
                primary_hunk_id: old_request.first_hunk_id().to_owned(),
            }],
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request: old_request,
        result,
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
    assert!(text.contains("Preparing the next part"));
    assert!(text.contains("Review out of date"));

    assert!(review.accept(ReviewCodexTaskResult {
        id,
        outcome: ReviewCodexOutcome::Failed("Codex stopped unexpectedly.".to_owned()),
        complete: true,
    }));
    let text = render_text(&review);
    assert!(text.contains("Old overview stays readable"));
    assert!(text.contains("Codex stopped unexpectedly"));
    assert!(text.contains("Regenerate review"));
}
