use ilmek_protocol::address::{Address, AddressError};

fn parse(text: &str) -> Result<Address, AddressError> {
    text.parse()
}

#[test]
fn well_formed_addresses_round_trip() {
    for text in [
        "ilmek://nuri/forum",
        "ilmek://ece/duyurular",
        "ilmek://a1/b-2/topics/17",
    ] {
        assert_eq!(parse(text).unwrap().to_string(), text);
    }
    let address = parse("ilmek://nuri/forum/topics/3").unwrap();
    assert_eq!(
        (
            address.namespace.as_str(),
            address.slug.as_str(),
            address.path.as_str()
        ),
        ("nuri", "forum", "topics/3")
    );
}

#[test]
fn bad_schemes_and_shapes_are_rejected() {
    assert_eq!(parse("http://nuri/forum"), Err(AddressError::Scheme));
    assert_eq!(parse("nuri/forum"), Err(AddressError::Scheme));
    assert_eq!(parse("ILMEK://nuri/forum"), Err(AddressError::Scheme));
    assert_eq!(parse("ilmek://nuri"), Err(AddressError::Shape));
    assert_eq!(parse("ilmek://"), Err(AddressError::Shape));
}

#[test]
fn names_follow_the_grammar() {
    for text in [
        "ilmek://Nuri/forum",
        "ilmek://nuri/Forum",
        "ilmek://-nuri/forum",
        "ilmek://nuri/forum-",
        "ilmek:///forum",
        "ilmek://nuri/",
        "ilmek://nu_ri/forum",
        "ilmek://türk/forum",
        "ilmek://nuri@host/forum",
        "ilmek://nuri:80/forum",
    ] {
        assert!(
            matches!(
                parse(text),
                Err(AddressError::Name(_) | AddressError::Shape)
            ),
            "{text}"
        );
    }
    let long = format!("ilmek://{}/forum", "a".repeat(49));
    assert!(parse(&long).is_err());
    assert!(parse(&format!("ilmek://{}/forum", "a".repeat(48))).is_ok());
}

#[test]
fn in_app_paths_cannot_escape_or_smuggle_controls() {
    for text in [
        "ilmek://nuri/forum/../x",
        "ilmek://nuri/forum/a/./b",
        "ilmek://nuri/forum/a\\b",
        "ilmek://nuri/forum/a\nb",
        "ilmek://nuri/forum/\u{0}",
    ] {
        assert_eq!(parse(text), Err(AddressError::Path), "{text:?}");
    }
    assert_eq!(
        parse(&format!("ilmek://nuri/forum/{}", "x".repeat(3000))),
        Err(AddressError::TooLong)
    );
}
