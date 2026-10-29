#[cfg(test)]
use cosmic::cosmic_config::ConfigSet;
use cosmic::cosmic_config::{Config, ConfigGet, Error};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const KEY: &str = "settings";
pub const DEFAULT_TEMPLATE: &str = include_str!("../resources/settings.ron");

/// Keep the library reader and template destination on the same XDG path.
#[derive(Clone, Debug)]
pub struct SettingsStore {
    config: Config,
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(id: &str, version: u64) -> Result<Self, String> {
        let root = dirs::config_dir().ok_or("Could not locate the user config directory")?;
        Self::with_root(id, version, &root)
    }

    fn with_root(id: &str, version: u64, root: &Path) -> Result<Self, String> {
        // Let Config validate the ID before using it to build the file path.
        let config = Config::with_custom_path(id, version, root.to_path_buf())
            .map_err(|err| err.to_string())?;
        let path = root
            .join("cosmic")
            .join(id)
            .join(format!("v{version}"))
            .join(KEY);
        Ok(Self { config, path })
    }

    fn create_template(&self) -> Result<(), String> {
        let parent = self.path.parent().ok_or("Invalid settings path")?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|err| err.to_string())?;
        file.write_all(DEFAULT_TEMPLATE.as_bytes())
            .map_err(|err| err.to_string())?;
        match file.persist_noclobber(&self.path) {
            Ok(_) => Ok(()),
            // Another applet instance may have created it while we were loading.
            Err(err) if err.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub duration_minutes: Vec<u32>,
    pub default_duration_minutes: Option<u32>,
    pub dangerous_enable_closed_lid_control: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            duration_minutes: vec![15, 30, 60],
            default_duration_minutes: None,
            dangerous_enable_closed_lid_control: false,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.default_duration_minutes == Some(0) {
            return Err("default_duration_minutes must be None or Some(positive minutes)".into());
        }
        if self.duration_minutes.is_empty() || self.duration_minutes.len() > 8 {
            return Err("duration_minutes must contain between 1 and 8 presets".into());
        }
        for (index, minutes) in self.duration_minutes.iter().enumerate() {
            if *minutes == 0 {
                return Err("duration_minutes values must be greater than zero".into());
            }
            if self.duration_minutes[..index].contains(minutes) {
                return Err("duration_minutes must not contain duplicates".into());
            }
        }
        Ok(())
    }

    pub fn load(store: &SettingsStore) -> Result<Self, String> {
        let settings = match store.config.get_local::<Self>(KEY) {
            Ok(settings) => settings,
            Err(Error::NotFound) => {
                store.create_template()?;
                // Read the winning file, which may belong to another instance.
                store
                    .config
                    .get_local::<Self>(KEY)
                    .map_err(|err| err.to_string())?
            }
            Err(err) => return Err(err.to_string()),
        };
        settings.validate()?;
        Ok(settings)
    }

    #[cfg(test)]
    pub fn save(&self, store: &SettingsStore) -> Result<(), String> {
        self.validate()?;
        store.config.set(KEY, self).map_err(|err| err.to_string())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    pub struct TestConfig {
        pub config: SettingsStore,
        pub path: PathBuf,
        root: PathBuf,
    }
    impl TestConfig {
        pub fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "caffeine-config-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            let config = SettingsStore::with_root("test.caffeine", 1, &root).unwrap();
            let path = root.join("cosmic/test.caffeine/v1/settings");
            Self { config, path, root }
        }
    }
    impl Drop for TestConfig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn missing_config_creates_editable_defaults_and_roundtrips_custom_settings() {
        let test = TestConfig::new();
        assert_eq!(Settings::load(&test.config).unwrap(), Settings::default());
        assert_eq!(
            std::fs::read_to_string(&test.path).unwrap(),
            DEFAULT_TEMPLATE
        );
        let settings = Settings {
            duration_minutes: vec![5, 25, 90, 240],
            default_duration_minutes: Some(25),
            dangerous_enable_closed_lid_control: true,
        };
        settings.save(&test.config).unwrap();
        assert_eq!(Settings::load(&test.config).unwrap(), settings);
    }

    #[test]
    fn invalid_config_is_reported_without_overwriting_user_file() {
        for contents in [
            "(duration_minutes: [0], dangerous_enable_closed_lid_control: false)",
            "(duration_minutes: [15, 15])",
            "(duration_minutes: [])",
            "(duration_minutes: [1,2,3,4,5,6,7,8,9])",
            "(duration_minutes: [-1])",
            "(duration_minutes: [25], dangerous_enable_closed_lid_control: \"yes\")",
            "(duration_mintes: [25])",
            "(default_duration_minutes: Some(0))",
            "broken RON",
        ] {
            let test = TestConfig::new();
            std::fs::write(&test.path, contents).unwrap();
            assert!(Settings::load(&test.config).is_err(), "{contents}");
            assert_eq!(std::fs::read_to_string(&test.path).unwrap(), contents);
        }
    }

    #[test]
    fn omitted_fields_use_defaults() {
        let test = TestConfig::new();
        std::fs::write(&test.path, "(duration_minutes: [20, 45])").unwrap();
        let settings = Settings::load(&test.config).unwrap();
        assert_eq!(settings.duration_minutes, vec![20, 45]);
        assert!(!settings.dangerous_enable_closed_lid_control);
    }

    #[test]
    fn failed_write_is_reported() {
        let test = TestConfig::new();
        std::fs::create_dir(&test.path).unwrap();
        assert!(Settings::default().save(&test.config).is_err());
    }

    #[test]
    fn existing_values_and_comments_are_preserved_even_during_creation_race() {
        let test = TestConfig::new();
        let contents = DEFAULT_TEMPLATE
            .replace(
                "duration_minutes: [15, 30, 60]",
                "duration_minutes: [30, 60, 240]",
            )
            .replace(
                "default_duration_minutes: None",
                "default_duration_minutes: Some(30)",
            )
            .replace(
                "dangerous_enable_closed_lid_control: false",
                "dangerous_enable_closed_lid_control: true",
            );
        std::fs::write(&test.path, &contents).unwrap();
        // Simulate a file appearing after a missing-file read in another instance.
        test.config.create_template().unwrap();
        let settings = Settings::load(&test.config).unwrap();
        assert_eq!(settings.duration_minutes, vec![30, 60, 240]);
        assert_eq!(settings.default_duration_minutes, Some(30));
        assert!(settings.dangerous_enable_closed_lid_control);
        assert_eq!(std::fs::read_to_string(&test.path).unwrap(), contents);
    }

    #[test]
    fn concurrent_first_launches_read_one_complete_commented_template() {
        let test = TestConfig::new();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let config = test.config.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    Settings::load(&config).unwrap()
                })
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap(), Settings::default());
        }
        assert_eq!(
            std::fs::read_to_string(&test.path).unwrap(),
            DEFAULT_TEMPLATE
        );
    }

    #[test]
    fn first_launch_write_failure_is_reported_without_replacing_existing_path() {
        let test = TestConfig::new();
        std::fs::create_dir(&test.path).unwrap();
        assert!(Settings::load(&test.config).is_err());
        assert!(test.path.is_dir());
    }
}
