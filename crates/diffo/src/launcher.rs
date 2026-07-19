use std::ffi::OsString;

use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LaunchMode {
    Application,
    Update,
}

pub(crate) fn dispatch(arguments: impl IntoIterator<Item = OsString>) -> Result<LaunchMode> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => Ok(LaunchMode::Application),
        [argument] if argument == "update" => Ok(LaunchMode::Update),
        _ => bail!("usage: diffo [update]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_application_and_fixed_update_entry_paths() {
        assert_eq!(dispatch([]).unwrap(), LaunchMode::Application);
        assert_eq!(
            dispatch([OsString::from("update")]).unwrap(),
            LaunchMode::Update
        );
        for arguments in [
            vec![OsString::from("--help")],
            vec![OsString::from("UPDATE")],
            vec![OsString::from("update"), OsString::from("extra")],
        ] {
            assert!(dispatch(arguments).is_err());
        }
    }
}
