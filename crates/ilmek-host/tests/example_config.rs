use ilmek_host::Settings;

#[test]
fn the_shipped_example_config_is_valid() {
    let settings = Settings::parse(include_str!("../../../configs/host.toml.example"))
        .expect("example host.toml must parse");
    assert_eq!(settings.listen.port(), 47402);
    assert_eq!(settings.advertised, None, "advertised is optional");
    assert_eq!(settings.advertised().port(), 47402);
    assert!(settings.advertised().ip().is_unspecified());
}
