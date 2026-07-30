use super::*;
use crate::diff::{file_group_areas, render_unpushed_commits};
use diffo_core::Commit;

fn model_with_unpushed(ahead: usize, commits: &[(&str, &str)]) -> Model {
    Model::new(RepositorySnapshot {
        head: HeadState::Named {
            name: "main".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        upstream: Some(UpstreamState {
            name: "origin/main".to_owned(),
            ahead,
            behind: 0,
            recent_local_commits: commits
                .iter()
                .map(|(id, summary)| Commit {
                    id: (*id).to_owned(),
                    summary: (*summary).to_owned(),
                })
                .collect(),
        }),
        ..RepositorySnapshot::default()
    })
}

fn model_without_upstream(commits: &[(&str, &str)]) -> Model {
    Model::new(RepositorySnapshot {
        head: HeadState::Named {
            name: "main".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        recent_commits: commits
            .iter()
            .map(|(id, summary)| Commit {
                id: (*id).to_owned(),
                summary: (*summary).to_owned(),
            })
            .collect(),
        ..RepositorySnapshot::default()
    })
}

#[test]
fn lists_three_commits_and_the_exact_remainder() {
    let model = model_with_unpushed(
        7,
        &[
            ("111111111", "Newest commit"),
            ("222222222", "Second commit"),
            ("333333333", "Third commit"),
        ],
    );
    let backend = TestBackend::new(28, 6);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| render_unpushed_commits(frame, frame.area(), &model))
        .unwrap();

    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("1111111 Newest commit"));
    assert!(text.contains("2222222 Second commit"));
    assert!(text.contains("3333333 Third commit"));
    assert!(text.contains("... and 4 more"));
}

#[test]
fn handles_unavailable_empty_and_unsafe_subjects() {
    let backend = TestBackend::new(22, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    let unavailable = Model::new(RepositorySnapshot::default());
    terminal
        .draw(|frame| render_unpushed_commits(frame, frame.area(), &unavailable))
        .unwrap();
    assert!(buffer_text(terminal.backend().buffer()).contains("No upstream"));

    let empty = model_with_unpushed(0, &[]);
    terminal
        .draw(|frame| render_unpushed_commits(frame, frame.area(), &empty))
        .unwrap();
    assert!(buffer_text(terminal.backend().buffer()).contains("No unpushed commits"));

    let unsafe_subject = model_with_unpushed(1, &[("abcdef012", "line one\nline two and long")]);
    terminal
        .draw(|frame| render_unpushed_commits(frame, frame.area(), &unsafe_subject))
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("abcdef0 line one␊"));
    assert!(text.contains('…'));
    assert!(!text.contains("line two"));
}

#[test]
fn treats_every_commit_as_unpushed_without_an_upstream() {
    let model = model_without_upstream(&[
        ("111111111", "Newest commit"),
        ("222222222", "Second commit"),
        ("333333333", "Third commit"),
        ("444444444", "Oldest commit"),
    ]);
    let backend = TestBackend::new(28, 6);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| render_unpushed_commits(frame, frame.area(), &model))
        .unwrap();

    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("1111111 Newest commit"));
    assert!(text.contains("2222222 Second commit"));
    assert!(text.contains("3333333 Third commit"));
    assert!(text.contains("... and more"));
    assert!(!text.contains("No upstream"));
}

#[test]
fn yields_height_to_both_file_groups() {
    let model = model_with_unpushed(7, &[("1", "one"), ("2", "two"), ("3", "three")]);

    let roomy = file_group_areas(Rect::new(0, 0, 20, 10), &model);
    assert_eq!(
        roomy.iter().map(|area| area.height).collect::<Vec<_>>(),
        [6, 2, 2]
    );

    let constrained = file_group_areas(Rect::new(0, 0, 20, 6), &model);
    assert_eq!(
        constrained
            .iter()
            .map(|area| area.height)
            .collect::<Vec<_>>(),
        [2, 2, 2]
    );

    let no_upstream =
        model_without_upstream(&[("1", "one"), ("2", "two"), ("3", "three"), ("4", "four")]);
    let no_upstream = file_group_areas(Rect::new(0, 0, 20, 10), &no_upstream);
    assert_eq!(
        no_upstream
            .iter()
            .map(|area| area.height)
            .collect::<Vec<_>>(),
        [6, 2, 2]
    );
}
