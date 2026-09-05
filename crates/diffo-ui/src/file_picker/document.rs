use crate::theme;
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
    pub destructive_action: Option<String>,
}

impl<K> Row<K> {
    pub fn flat(id: K, label: Line<'static>) -> Self {
        Self {
            id,
            label,
            action: None,
            context_menu: true,
            destructive_action: None,
        }
    }

    #[must_use]
    pub fn with_action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }

    #[must_use]
    pub fn with_destructive_action(mut self, label: impl Into<String>) -> Self {
        self.destructive_action = Some(label.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct TreeNode<K> {
    id: K,
    label: Line<'static>,
    branch: bool,
    context_menu: bool,
    children: Vec<Self>,
}

impl<K> TreeNode<K> {
    pub fn leaf(id: K, label: Line<'static>) -> Self {
        Self {
            id,
            label,
            branch: false,
            context_menu: true,
            children: Vec::new(),
        }
    }

    pub fn branch(id: K, label: Line<'static>, children: Vec<Self>) -> Self {
        Self {
            id,
            label,
            branch: true,
            context_menu: false,
            children,
        }
    }

    #[must_use]
    pub const fn with_context_menu(mut self) -> Self {
        self.context_menu = true;
        self
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
    pub(crate) destructive_action: Option<String>,
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
                    destructive_action: row.destructive_action,
                })
                .collect(),
            panel_action: None,
            empty_message: "No files.".to_owned(),
            border_style: theme::chrome_style(),
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
            border_style: theme::chrome_style(),
        }
    }
}

fn append_tree_rows<K>(rows: &mut Vec<DocumentRow<K>>, node: TreeNode<K>, depth: usize) {
    let TreeNode {
        id,
        label,
        branch,
        context_menu,
        children,
    } = node;
    rows.push(DocumentRow {
        id,
        label,
        depth,
        branch,
        action: None,
        context_menu,
        destructive_action: None,
    });
    for child in children {
        append_tree_rows(rows, child, depth.saturating_add(1));
    }
}
