use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet, Error};
use serde::{Deserialize, Serialize};

pub const KEY: &str = "settings";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub dangerous_enable_closed_lid_control: bool,
}

impl Settings {
    pub fn load(config: &Config) -> Result<Self, String> {
        match config.get_local::<Self>(KEY) {
            Ok(settings) => Ok(settings),
            Err(Error::NotFound) => {
                let defaults = Self::default();
                defaults.save(config)?;
                Ok(defaults)
            }
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn save(&self, config: &Config) -> Result<(), String> {
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
            dangerous_enable_closed_lid_control: true,
        };
        settings.save(&test.config).unwrap();
        assert_eq!(Settings::load(&test.config).unwrap(), settings);
    }

    #[test]
    fn invalid_config_is_reported_without_overwriting_user_file() {
        for contents in [
            "(dangerous_enable_closed_lid_control: \"yes\")",
            "(dangerous_enable_closed_lid_contol: true)",
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
        std::fs::write(&test.path, "()").unwrap();
        let settings = Settings::load(&test.config).unwrap();
        assert!(!settings.dangerous_enable_closed_lid_control);
    }

    #[test]
    fn failed_write_is_reported() {
        let test = TestConfig::new();
        std::fs::create_dir(&test.path).unwrap();
        assert!(Settings::default().save(&test.config).is_err());
    }
}
