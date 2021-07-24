pub fn version_semver() -> semver::Version{
    semver::Version::parse(VERSION_STRING).unwrap()
}

pub const VERSION_STRING: &str = "0.2.0";
