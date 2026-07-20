pub(super) fn find_panel_action(
    cells: &[Vec<String>],
    panel: &str,
    action: &str,
) -> Vec<(u16, u16)> {
    cells
        .iter()
        .enumerate()
        .filter(|(_, row)| !find_in_row(row, panel).is_empty())
        .flat_map(|(row, cells)| positions(row, find_in_row(cells, action), action))
        .collect()
}

pub(super) fn find_file_action(
    cells: &[Vec<String>],
    panel: &str,
    path: &str,
    action: &str,
) -> Vec<(u16, u16)> {
    let Some(panel_row) = cells
        .iter()
        .position(|row| !find_in_row(row, panel).is_empty())
    else {
        return Vec::new();
    };
    let end = cells
        .iter()
        .enumerate()
        .skip(panel_row + 1)
        .find(|(_, row)| {
            !find_in_row(row, "Staged").is_empty() || !find_in_row(row, "Changes").is_empty()
        })
        .map_or(cells.len(), |(row, _)| row);
    cells
        .iter()
        .enumerate()
        .take(end)
        .skip(panel_row + 1)
        .filter(|(_, row)| !find_in_row(row, path).is_empty())
        .flat_map(|(row, cells)| positions(row, find_in_row(cells, action), action))
        .collect()
}

pub(super) fn find_dialog_action(
    cells: &[Vec<String>],
    dialog: &str,
    action: &str,
) -> Vec<(u16, u16)> {
    if find_text(cells, dialog).is_empty() {
        return Vec::new();
    }
    let label = match action {
        "Commit" => "[ Commit (Enter) ]".to_owned(),
        "Cancel" => "[ Cancel (Esc) ]".to_owned(),
        _ => format!("[ {action} ]"),
    };
    cells
        .iter()
        .enumerate()
        .filter(|(_, row)| !find_in_row(row, "[ Cancel (Esc) ]").is_empty())
        .flat_map(|(row, cells)| positions(row, find_in_row(cells, &label), &label))
        .collect()
}

pub(super) fn find_toast_action(
    cells: &[Vec<String>],
    toast: &str,
    action: &str,
) -> Vec<(u16, u16)> {
    cells
        .iter()
        .enumerate()
        .filter(|(_, row)| !find_in_row(row, toast).is_empty())
        .flat_map(|(toast_row, _)| {
            toast_row.saturating_sub(1)
                ..=toast_row
                    .saturating_add(1)
                    .min(cells.len().saturating_sub(1))
        })
        .flat_map(|row| positions(row, find_in_row(&cells[row], action), action))
        .collect()
}

pub(super) fn find_text(cells: &[Vec<String>], text: &str) -> Vec<(u16, u16)> {
    cells
        .iter()
        .enumerate()
        .flat_map(|(row, cells)| positions(row, find_in_row(cells, text), text))
        .collect()
}

pub(super) fn positions(row: usize, starts: Vec<usize>, text: &str) -> Vec<(u16, u16)> {
    let center = text.chars().count() / 2;
    starts
        .into_iter()
        .filter_map(|column| {
            Some((
                u16::try_from(column.checked_add(center)?).ok()?,
                u16::try_from(row).ok()?,
            ))
        })
        .collect()
}

pub(super) fn find_in_row(cells: &[String], text: &str) -> Vec<usize> {
    let needle = text
        .chars()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    if needle.is_empty() || needle.len() > cells.len() {
        return Vec::new();
    }
    cells
        .windows(needle.len())
        .enumerate()
        .filter_map(|(index, window)| (window == needle).then_some(index))
        .collect()
}
