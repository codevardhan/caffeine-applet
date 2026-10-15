use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet, Error};
use serde::{Deserialize, Serialize};

pub const KEY: &str = "settings";

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

    pub fn load(config: &Config) -> Result<Self, String> {
        let settings = match config.get_local::<Self>(KEY) {
            Ok(settings) => settings,
            Err(Error::NotFound) => {
                let defaults = Self::default();
                defaults.save(config)?;
                defaults
            }
            Err(err) => return Err(err.to_string()),
        };
        settings.validate()?;
        Ok(settings)
    }

    pub fn save(&self, config: &Config) -> Result<(), String> {
        self.validate()?;
        config.set(KEY, self).map_err(|err| err.to_string())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    pub struct TestConfig {
        pub config: Config,
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
            let config = Config::with_custom_path("test.caffeine", 1, root.clone()).unwrap();
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
        assert!(test.path.exists());
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
}
