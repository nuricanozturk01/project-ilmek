use ilmek_registry::Settings;

#[test]
fn the_shipped_example_config_is_valid() {
    let settings = Settings::parse(include_str!("../../../configs/registry.toml.example"))
        .expect("example registry.toml must parse");
    assert_eq!(settings.control_listen.port(), 47401);
    assert_eq!(settings.enroll_listen.port(), 47402);
    assert!(!settings.allowed_endpoints.is_empty());
}
