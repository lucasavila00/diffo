use super::*;
use crossterm::event::{KeyEvent, KeyModifiers};

fn type_query(picker: &mut SearchPicker<i32, i32>, query: &str) {
    for character in query.chars() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        let _ = picker.handle_event(&event, Rect::new(0, 0, 100, 30));
    }
}

#[test]
fn aliases_rank_stably_and_disabled_rows_cannot_activate() {
    let mut picker = SearchPicker::new("Branches", "None");
    picker.set_items(vec![
        SearchItem {
            identity: 1,
            payload: "main-a",
            label: "main".to_owned(),
            preferred_match: None,
            trailing: None,
            aliases: Vec::new(),
            enabled: false,
        },
        SearchItem {
            identity: 2,
            payload: "topic-a",
            label: "topic".to_owned(),
            preferred_match: None,
            trailing: None,
            aliases: Vec::new(),
            enabled: true,
        },
        SearchItem {
            identity: 3,
            payload: "remote-topic-a",
            label: "origin/topic".to_owned(),
            preferred_match: None,
            trailing: None,
            aliases: vec!["topic".to_owned()],
            enabled: true,
        },
    ]);

    assert_eq!(picker.selected_identity(), Some(&2));
    let area = Rect::new(0, 0, 80, 24);
    let event = Event::Key(KeyEvent::new(KeyCode::Char('T'), KeyModifiers::NONE));
    let _ = picker.handle_event(&event, area);
    assert_eq!(picker.query(), "T");
    assert_eq!(picker.selected_identity(), Some(&2));
    let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        picker.handle_event(&enter, area),
        SearchPickerEvent::Activate("topic-a")
    );
}

#[test]
fn shared_fuzzy_ranking_prefers_the_intended_contiguous_file_match() {
    let item = |identity, label: &str| SearchItem {
        identity,
        payload: identity,
        label: label.to_owned(),
        preferred_match: None,
        trailing: None,
        aliases: Vec::new(),
        enabled: true,
    };
    let mut picker = SearchPicker::new("Files", "None");
    picker.set_items(vec![
        item(1, "target/doc/diffo_ui/file_picker/fn.help_rows.html"),
        item(
            2,
            "target/doc/diffo_ui/search_picker/fn.search_picker_layout.html",
        ),
        item(3, ".devcontainer/Dockerfile"),
    ]);
    type_query(&mut picker, "Dockerf");

    assert_eq!(picker.selected_identity(), Some(&3));
    assert_eq!(
        picker
            .matches()
            .into_iter()
            .map(|item| item.identity)
            .collect::<Vec<_>>(),
        vec![3, 1, 2]
    );
}

#[test]
fn preferred_match_tier_ranks_a_file_name_above_a_folder_match() {
    let item = |identity, label: &str, file_name: &str| SearchItem {
        identity,
        payload: identity,
        label: label.to_owned(),
        preferred_match: Some(file_name.to_owned()),
        trailing: None,
        aliases: Vec::new(),
        enabled: true,
    };
    let mut picker = SearchPicker::new("Files", "None");
    picker.set_items(vec![
        item(1, "query/unrelated.rs", "unrelated.rs"),
        item(2, "src/query.rs", "query.rs"),
    ]);
    type_query(&mut picker, "query");

    assert_eq!(picker.selected_identity(), Some(&2));
}

#[test]
fn trims_whitespace_from_typed_and_pasted_queries() {
    let item = SearchItem {
        identity: 1,
        payload: 1,
        label: "origin/wt/track-codex-git-state".to_owned(),
        preferred_match: None,
        trailing: None,
        aliases: Vec::new(),
        enabled: true,
    };
    let area = Rect::new(0, 0, 100, 30);
    let query = "/wt/track-codex-git-state";

    let mut typed = SearchPicker::new("Branches", "None");
    typed.set_items(vec![item.clone()]);
    type_query(&mut typed, &format!(" {query} "));
    assert_eq!(typed.query(), query);
    assert_eq!(typed.selected_identity(), Some(&1));

    let mut pasted = SearchPicker::new("Branches", "None");
    pasted.set_items(vec![item]);
    let _ = pasted.handle_event(&Event::Paste(format!(" \n{query}\t")), area);
    assert_eq!(pasted.query(), query);
    assert_eq!(pasted.selected_identity(), Some(&1));
}
