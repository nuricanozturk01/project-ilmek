//! `ilmek://namespace/slug[/path]` application addresses (REQUIREMENTS §12).

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::wire::Name;

const SCHEME: &str = "ilmek://";
const MAX_ADDRESS_BYTES: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AddressError {
    #[error("address must start with ilmek://")]
    Scheme,
    #[error("address must name a publisher and an application: ilmek://publisher/app")]
    Shape,
    #[error("`{0}` is not a valid name (lowercase ASCII letters, digits and hyphens)")]
    Name(String),
    #[error("the in-app path must be relative and free of dot segments, backslashes and controls")]
    Path,
    #[error("address is longer than {MAX_ADDRESS_BYTES} bytes")]
    TooLong,
}

/// A parsed application address. The path is in-app navigation only; it is never a file path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    pub namespace: Name,
    pub slug: Name,
    /// In-app location, without a leading slash; empty for the app's entry point.
    pub path: String,
}

impl FromStr for Address {
    type Err = AddressError;

    fn from_str(text: &str) -> Result<Self, AddressError> {
        if text.len() > MAX_ADDRESS_BYTES {
            return Err(AddressError::TooLong);
        }
        let rest = text.strip_prefix(SCHEME).ok_or(AddressError::Scheme)?;
        let mut parts = rest.splitn(3, '/');
        let (Some(namespace), Some(slug)) = (parts.next(), parts.next()) else {
            return Err(AddressError::Shape);
        };
        let path = parts.next().unwrap_or_default();
        let valid_path = path
            .split('/')
            .all(|segment| segment != "." && segment != "..")
            && !path.contains('\\')
            && !path.chars().any(char::is_control);
        if !valid_path {
            return Err(AddressError::Path);
        }
        Ok(Self {
            namespace: namespace
                .parse()
                .map_err(|_| AddressError::Name(namespace.to_owned()))?,
            slug: slug
                .parse()
                .map_err(|_| AddressError::Name(slug.to_owned()))?,
            path: path.to_owned(),
        })
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SCHEME}{}/{}", self.namespace, self.slug)?;
        if !self.path.is_empty() {
            write!(f, "/{}", self.path)?;
        }
        Ok(())
    }
}
