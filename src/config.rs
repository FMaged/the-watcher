//! Configuration, read once from the environment at startup.
//!
//! Two rules shape this module. Everything comes from the environment, so no secret is ever
//! committed (N-40). And a missing or unparsable value stops the process here rather than
//! surfacing at three in the morning on the night it is first needed.

use std::env::{self, VarError};
use std::fmt;

#[derive(Clone)]
pub struct Config {
    /// Connects as the restricted application role, which holds no UPDATE or DELETE on the
    /// append-only tables (D-12).
    pub database_url: Secret,
    /// Connects as the owner role. Used by migrations and by nothing else.
    pub migrate_database_url: Secret,

    pub model_provider: String,
    pub model_name: String,
    pub model_base_url: String,
    pub model_api_key: Secret,

    pub price_source: String,
    pub news_source: String,

    /// N-30, checked before a run starts rather than after it finishes (N-34).
    pub daily_cost_cap_cents: u64,
    /// F-53.
    pub chat_cap_per_day: u32,

    /// C-19. Both configurable, both to be revisited once there is real data.
    pub half_spread_bps: u32,
    pub fee_cents: u64,
    /// C-20. A starting value chosen to be revised, not a considered one.
    pub annotation_warn_pct: u32,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let config = Self {
            database_url: Secret(required("DATABASE_URL")?),
            migrate_database_url: Secret(required("MIGRATE_DATABASE_URL")?),

            model_provider: required("MODEL_PROVIDER")?,
            model_name: required("MODEL_NAME")?,
            model_base_url: required("MODEL_BASE_URL")?,
            model_api_key: Secret(required("MODEL_API_KEY")?),

            price_source: required("PRICE_SOURCE")?,
            news_source: required("NEWS_SOURCE")?,

            daily_cost_cap_cents: parsed("DAILY_COST_CAP_CENTS")?,
            chat_cap_per_day: parsed("CHAT_CAP_PER_DAY")?,

            half_spread_bps: parsed("HALF_SPREAD_BPS")?,
            fee_cents: parsed("FEE_CENTS")?,
            annotation_warn_pct: parsed("ANNOTATION_WARN_PCT")?,
        };
        config.validate()?;
        Ok(config)
    }

    /// Catch what is cheap to catch now. A mistyped connection string otherwise surfaces at
    /// the first query, which on a scheduled run means the following night.
    fn validate(&self) -> Result<(), ConfigError> {
        for (key, url) in [
            ("DATABASE_URL", &self.database_url),
            ("MIGRATE_DATABASE_URL", &self.migrate_database_url),
        ] {
            let scheme_ok = url.expose().starts_with("postgres://")
                || url.expose().starts_with("postgresql://");
            if !scheme_ok {
                return Err(ConfigError::Unparsable {
                    key,
                    expected: "a postgres:// connection string",
                });
            }
        }
        if self.annotation_warn_pct == 0 || self.annotation_warn_pct > 100 {
            return Err(ConfigError::Unparsable {
                key: "ANNOTATION_WARN_PCT",
                expected: "a percentage between 1 and 100",
            });
        }
        Ok(())
    }
}

/// A value that must not reach a log, a browser or an error message (N-40).
///
/// The wrapper exists because `Config` is the kind of struct someone eventually prints while
/// debugging. Deriving `Debug` on the plain `String` would put a database password and a model
/// key in the log line, and nothing would complain.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &self.database_url)
            .field("migrate_database_url", &self.migrate_database_url)
            .field("model_provider", &self.model_provider)
            .field("model_name", &self.model_name)
            .field("model_base_url", &self.model_base_url)
            .field("model_api_key", &self.model_api_key)
            .field("price_source", &self.price_source)
            .field("news_source", &self.news_source)
            .field("daily_cost_cap_cents", &self.daily_cost_cap_cents)
            .field("chat_cap_per_day", &self.chat_cap_per_day)
            .field("half_spread_bps", &self.half_spread_bps)
            .field("fee_cents", &self.fee_cents)
            .field("annotation_warn_pct", &self.annotation_warn_pct)
            .finish()
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Missing(&'static str),
    NotUnicode(&'static str),
    /// Carries the variable name but never its value, which may be a secret.
    Unparsable {
        key: &'static str,
        expected: &'static str,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(key) => write!(f, "{key} is not set"),
            Self::NotUnicode(key) => write!(f, "{key} is not valid unicode"),
            Self::Unparsable { key, expected } => write!(f, "{key} is not {expected}"),
        }
    }
}

impl std::error::Error for ConfigError {}

fn required(key: &'static str) -> Result<String, ConfigError> {
    match env::var(key) {
        Ok(v) if v.trim().is_empty() => Err(ConfigError::Missing(key)),
        Ok(v) => Ok(v),
        Err(VarError::NotPresent) => Err(ConfigError::Missing(key)),
        Err(VarError::NotUnicode(_)) => Err(ConfigError::NotUnicode(key)),
    }
}

fn parsed<T>(key: &'static str) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    required(key)?
        .trim()
        .parse()
        .map_err(|_| ConfigError::Unparsable {
            key,
            expected: std::any::type_name::<T>(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENSITIVE: &str = "postgres://watcher_app:hunter2@db/the_watcher";

    #[test]
    fn debug_redacts_a_secret() {
        let s = Secret(SENSITIVE.to_string());
        let shown = format!("{s:?}");
        assert_eq!(shown, "<redacted>");
        assert!(!shown.contains("hunter2"));
    }

    #[test]
    fn display_redacts_a_secret() {
        let s = Secret(SENSITIVE.to_string());
        assert!(!format!("{s}").contains("hunter2"));
    }

    #[test]
    fn expose_returns_the_value() {
        // The only way to read a secret is to ask for it by name, which is grep-able.
        assert_eq!(Secret(SENSITIVE.to_string()).expose(), SENSITIVE);
    }

    #[test]
    fn a_config_debug_line_carries_no_secret() {
        // The case this guards: someone adds a field, derives nothing, and prints the struct
        // while debugging. N-40 says that line must be safe to paste into an issue.
        let c = Config {
            database_url: Secret(SENSITIVE.to_string()),
            migrate_database_url: Secret(SENSITIVE.to_string()),
            model_provider: "anthropic".into(),
            model_name: "a-model".into(),
            model_base_url: "https://example.invalid".into(),
            model_api_key: Secret("sk-secret-key".into()),
            price_source: "free".into(),
            news_source: "none".into(),
            daily_cost_cap_cents: 100,
            chat_cap_per_day: 50,
            half_spread_bps: 5,
            fee_cents: 100,
            annotation_warn_pct: 25,
        };
        let shown = format!("{c:?}");
        assert!(!shown.contains("hunter2"), "database password leaked");
        assert!(!shown.contains("sk-secret-key"), "model key leaked");
        assert!(
            shown.contains("a-model"),
            "non-secret fields should still show"
        );
    }

    fn valid() -> Config {
        Config {
            database_url: Secret(SENSITIVE.to_string()),
            migrate_database_url: Secret(SENSITIVE.to_string()),
            model_provider: "anthropic".into(),
            model_name: "a-model".into(),
            model_base_url: "https://example.invalid".into(),
            model_api_key: Secret("sk-secret-key".into()),
            price_source: "free".into(),
            news_source: "none".into(),
            daily_cost_cap_cents: 100,
            chat_cap_per_day: 50,
            half_spread_bps: 5,
            fee_cents: 100,
            annotation_warn_pct: 25,
        }
    }

    #[test]
    fn a_valid_config_passes_validation() {
        assert!(valid().validate().is_ok());
    }

    #[test]
    fn a_database_url_with_the_wrong_scheme_is_refused() {
        let mut c = valid();
        c.database_url = Secret("mysql://watcher@db/the_watcher".into());
        let e = c.validate().expect_err("wrong scheme should not pass");
        assert!(e.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn a_nonsense_warning_threshold_is_refused() {
        let mut c = valid();
        c.annotation_warn_pct = 0;
        assert!(c.validate().is_err());
        c.annotation_warn_pct = 101;
        assert!(c.validate().is_err());
    }

    #[test]
    fn a_blank_required_variable_is_missing_rather_than_empty() {
        const KEY: &str = "WATCHER_TEST_REQUIRED_ONE";
        std::env::set_var(KEY, "  ");
        assert!(matches!(required(KEY), Err(ConfigError::Missing(_))));
        std::env::remove_var(KEY);
    }

    #[test]
    fn an_error_names_the_variable_but_not_its_value() {
        let e = ConfigError::Unparsable {
            key: "FEE_CENTS",
            expected: "u64",
        };
        let shown = e.to_string();
        assert!(shown.contains("FEE_CENTS"));
        assert!(!shown.contains("hunter2"));
    }
}
