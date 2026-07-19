use diffo_ui::theme;
use ratatui::{style::Style, text::Line};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Flat,
    Tree,
}

#[derive(Clone, Debug)]
pub struct Row<K> {
    pub id: K,
    pub label: Line<'static>,
    pub action: Option<String>,
    pub context_menu: bool,
}

impl<K> Row<K> {
    pub fn flat(id: K, label: Line<'static>) -> Self {
        Self {
            id,
            label,
            action: None,
            context_menu: true,
        }
    }

    #[must_use]
    pub fn with_action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct TreeNode<K> {
    id: K,
    label: Line<'static>,
    branch: bool,
    children: Vec<Self>,
}

impl<K> TreeNode<K> {
    pub fn leaf(id: K, label: Line<'static>) -> Self {
        Self {
            id,
            label,
            branch: false,
            children: Vec::new(),
        }
    }

    pub fn branch(id: K, label: Line<'static>, children: Vec<Self>) -> Self {
        Self {
            id,
            label,
            branch: true,
            children,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DocumentRow<K> {
    pub(crate) id: K,
    pub(crate) label: Line<'static>,
    pub(crate) depth: usize,
    pub(crate) branch: bool,
    pub(crate) action: Option<String>,
    pub(crate) context_menu: bool,
}

#[derive(Clone, Debug)]
pub struct Document<K> {
    pub title: String,
    pub(crate) mode: Mode,
    pub(crate) rows: Vec<DocumentRow<K>>,
    pub panel_action: Option<String>,
    pub empty_message: String,
    pub border_style: Style,
}

impl<K> Document<K> {
    pub fn flat(title: impl Into<String>, rows: Vec<Row<K>>) -> Self {
        Self {
            title: title.into(),
            mode: Mode::Flat,
            rows: rows
                .into_iter()
                .map(|row| DocumentRow {
                    id: row.id,
                    label: row.label,
                    depth: 0,
                    branch: false,
                    action: row.action,
                    context_menu: row.context_menu,
                })
                .collect(),
            panel_action: None,
            empty_message: "No files.".to_owned(),
            border_style: Style::default().fg(theme::CHROME),
        }
    }

    pub fn tree(title: impl Into<String>, nodes: Vec<TreeNode<K>>) -> Self {
        let mut rows = Vec::new();
        for node in nodes {
            append_tree_rows(&mut rows, node, 0);
        }
        Self {
            title: title.into(),
            mode: Mode::Tree,
            rows,
            panel_action: None,
            empty_message: "No files.".to_owned(),
            border_style: Style::default().fg(theme::CHROME),
        }
    }
}

fn append_tree_rows<K>(rows: &mut Vec<DocumentRow<K>>, node: TreeNode<K>, depth: usize) {
    let TreeNode {
        id,
        label,
        branch,
        children,
    } = node;
    rows.push(DocumentRow {
        id,
        label,
        depth,
        branch,
        action: None,
        context_menu: !branch,
    });
    for child in children {
        append_tree_rows(rows, child, depth.saturating_add(1));
    }
}
