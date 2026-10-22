use ilmek_protocol::Uuid7;

#[test]
fn generated_ids_round_trip_and_are_version_7() {
    let id = Uuid7::generate();
    let text = id.to_string();
    assert_eq!(text.parse::<Uuid7>().unwrap(), id);
    assert_eq!(text.as_bytes()[14], b'7');
    assert!(matches!(text.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
}

#[test]
fn rejects_other_versions_nil_and_non_canonical_forms() {
    for text in [
        "00000000-0000-0000-0000-000000000000",
        "f47ac10b-58cc-4372-a567-0e02b2c3d479",
        "01A160E8-5064-78B9-9504-AF4963FA6DF3",
        "01a160e850647 8b99504af4963fa6df3",
        "01a160e8506478b99504af4963fa6df3",
        "{01a160e8-5064-78b9-9504-af4963fa6df3}",
        "urn:uuid:01a160e8-5064-78b9-9504-af4963fa6df3",
        "01a160e8-5064-78b9-c504-af4963fa6df3",
        "",
    ] {
        assert!(text.parse::<Uuid7>().is_err(), "{text:?}");
    }
}

#[test]
fn accepts_fixture_ids() {
    assert!(
        "01a160e8-5064-78b9-9504-af4963fa6df3"
            .parse::<Uuid7>()
            .is_ok()
    );
}
