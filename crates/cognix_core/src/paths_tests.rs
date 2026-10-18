use dirs::home_dir;

use super::*;

#[test]
fn test_data_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    // ChannelState, by default, is configured for Channel::Oss.
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(data_dir(), home_dir.join(".cognix-oss"));
        } else if #[cfg(target_os = "linux")] {
            assert_eq!(data_dir(), home_dir.join(".local/share/cognix-oss"));
        } else if #[cfg(windows)] {
            assert_eq!(data_dir(), home_dir.join("AppData\\Roaming\\cognix\\CognixOss\\data"));
        } else {
            panic!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_config_local_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    // ChannelState, by default, is configured for Channel::Oss.
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(config_local_dir(), home_dir.join(".cognix-oss"));
        } else if #[cfg(target_os = "linux")] {
            assert_eq!(config_local_dir(), home_dir.join(".config/cognix-oss"));
        } else if #[cfg(windows)] {
            assert_eq!(config_local_dir(), home_dir.join("AppData\\Local\\cognix\\CognixOss\\config"));
        } else {
            panic!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_cognix_home_config_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    let expected_dir_name = match ChannelState::data_profile() {
        Some(data_profile) => format!(".cognix-oss-{data_profile}"),
        None => ".cognix-oss".to_string(),
    };

    assert_eq!(
        cognix_home_config_dir(),
        Some(home_dir.join(expected_dir_name))
    );
}

#[test]
fn test_cognix_home_skills_and_mcp_paths() {
    let Some(config_dir) = cognix_home_config_dir() else {
        panic!("Should be able to compute Cognix home config directory");
    };

    assert_eq!(cognix_home_skills_dir(), Some(config_dir.join("skills")));
    assert_eq!(
        cognix_home_mcp_config_file_path(),
        Some(config_dir.join(".mcp.json"))
    );
}
#[test]
fn test_cache_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    // ChannelState, by default, is configured for Channel::Oss.
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(cache_dir(), home_dir.join("Library/Application Support/dev.cognix.CognixOss"));
        } else if #[cfg(target_os = "linux")] {
            assert_eq!(cache_dir(), home_dir.join(".cache/cognix-oss"));
        } else if #[cfg(windows)] {
            assert_eq!(cache_dir(), home_dir.join("AppData\\Local\\cognix\\CognixOss\\cache"));
        } else {
            panic!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_state_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    cfg_if::cfg_if! {
        // ChannelState, by default, is configured for Channel::Oss.
        if #[cfg(target_os = "macos")] {
            assert_eq!(state_dir(), home_dir.join("Library/Application Support/dev.cognix.CognixOss"));
        } else if #[cfg(target_os = "linux")] {
            assert_eq!(state_dir(), home_dir.join(".local/state/cognix-oss"));
        } else if #[cfg(windows)] {
            assert_eq!(state_dir(), home_dir.join("AppData\\Local\\cognix\\CognixOss\\data"));
        } else {
            panic!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_project_path_for_cognix_app_id() {
    let project_dirs = project_dirs_for_app_id(AppId::new("dev", "cognix", "Cognix"), None)
        .expect("should be able to compute project dirs");
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(project_dirs.project_path(), "dev.cognix.Cognix");
        } else if #[cfg(target_os = "linux")] {
            assert_eq!(project_dirs.project_path(), "cognix-terminal");
        } else if #[cfg(windows)] {
            assert_eq!(project_dirs.project_path(), "cognix\\Cognix");
        } else {
            panic!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_project_path_for_cognix_dev_app_id() {
    let project_dirs = project_dirs_for_app_id(AppId::new("dev", "cognix", "CognixDev"), None)
        .expect("should be able to compute project dirs");
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(project_dirs.project_path(), "dev.cognix.CognixDev");
        } else if #[cfg(target_os = "linux")] {
            assert_eq!(project_dirs.project_path(), "cognix-terminal-dev");
        } else if #[cfg(windows)] {
            assert_eq!(project_dirs.project_path(), "cognix\\CognixDev");
        } else {
            panic!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_project_path_for_oss_app_id() {
    let project_dirs = project_dirs_for_app_id(AppId::new("dev", "cognix", "CognixOss"), None)
        .expect("should be able to compute project dirs");
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(project_dirs.project_path(), "dev.cognix.CognixOss");
        } else if #[cfg(target_os = "linux")] {
            assert_eq!(project_dirs.project_path(), "cognix-oss");
        } else if #[cfg(windows)] {
            assert_eq!(project_dirs.project_path(), "cognix\\CognixOss");
        } else {
            panic!("Need to update tests for current platform!");
        }
    }
}
