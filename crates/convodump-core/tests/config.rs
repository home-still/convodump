// figment::Jail dictates the large `figment::Error` closure return type.
#![allow(clippy::result_large_err)]

use convodump_core::config::{load_from, CliOverrides};
use figment::Jail;

#[test]
fn env_overrides_file_and_cli_overrides_env() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.yaml",
            "client:\n  server_url: http://file.invalid:1\n  token: from-file\n",
        )?;
        let file = jail.directory().join("config.yaml");

        let cfg = load_from(&file, CliOverrides::default()).map_err(|e| e.to_string())?;
        assert_eq!(cfg.client.server_url, "http://file.invalid:1");
        assert_eq!(cfg.client.token, "from-file");

        jail.set_env(
            "HOME_STILL_CONVODUMP_CLIENT__SERVER_URL",
            "http://env.invalid:2",
        );
        let cfg = load_from(&file, CliOverrides::default()).map_err(|e| e.to_string())?;
        assert_eq!(cfg.client.server_url, "http://env.invalid:2");
        assert_eq!(cfg.client.token, "from-file");

        let cli = CliOverrides {
            server_url: Some("http://cli.invalid:3".into()),
            bind: None,
        };
        let cfg = load_from(&file, cli).map_err(|e| e.to_string())?;
        assert_eq!(cfg.client.server_url, "http://cli.invalid:3");
        Ok(())
    });
}

#[test]
fn unknown_key_is_an_error() {
    Jail::expect_with(|jail| {
        jail.create_file("config.yaml", "client:\n  serv_url: oops\n")?;
        let file = jail.directory().join("config.yaml");
        assert!(load_from(&file, CliOverrides::default()).is_err());
        Ok(())
    });
}

#[test]
fn missing_file_gives_defaults() {
    Jail::expect_with(|jail| {
        let file = jail.directory().join("absent.yaml");
        let cfg = load_from(&file, CliOverrides::default()).map_err(|e| e.to_string())?;
        assert_eq!(cfg.client.server_url, "http://127.0.0.1:7450");
        assert_eq!(cfg.server.max_upload_mb, 8192);
        assert!(cfg.engine.wrapper.is_empty());
        Ok(())
    });
}

#[test]
fn default_yaml_matches_compiled_defaults() {
    use convodump_core::config::ConvodumpConfig;
    use figment::providers::{Format, Yaml};
    let parsed: ConvodumpConfig = figment::Figment::new()
        .merge(Yaml::string(include_str!("../../../config/default.yaml")))
        .extract()
        .unwrap();
    assert_eq!(parsed, ConvodumpConfig::default());
}
