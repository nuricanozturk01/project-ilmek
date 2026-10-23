use ilmek_host::Settings;

#[test]
fn the_shipped_example_config_is_valid() {
    let settings = Settings::parse(include_str!("../../../configs/host.toml.example"))
        .expect("example host.toml must parse");
    assert_eq!(settings.listen, settings.advertised);
}
