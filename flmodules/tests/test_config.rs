use bytes::Bytes;
use flmacro::VersionedSerde;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

#[serde_as]
#[derive(VersionedSerde, Clone, PartialEq, Debug)]
#[versions = "[ConfigV1, ConfigV2, ConfigV3]"]
struct Config {
    #[serde_as(as = "Base64")]
    name4: Bytes,
}

impl From<ConfigV3> for Config {
    fn from(value: ConfigV3) -> Self {
        Self { name4: value.name3 }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct ConfigV3 {
    name3: Bytes,
}

impl From<ConfigV2> for ConfigV3 {
    fn from(value: ConfigV2) -> Self {
        Self { name3: value.name2 }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct ConfigV2 {
    name2: Bytes,
}

impl From<ConfigV1> for ConfigV2 {
    fn from(value: ConfigV1) -> Self {
        Self { name2: value.name }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct ConfigV1 {
    name: Bytes,
}

#[test]
fn test_config() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate the storage of an old configuration.
    let v1 = ConfigV1 { name: "123".into() };
    let cv1 = ConfigVersion::V1(v1);
    let c: Config = cv1.clone().into();
    let cv1_str = serde_yaml::to_string(&cv1)?;

    // Now the old version is recovered, and automatically converted
    // to the latest version.
    let c_recover: Config = serde_yaml::from_str(&cv1_str)?;
    assert_eq!(c_recover, cv1.into());

    // Storing and retrieving the latest version is always
    // done using the original struct, `Config` in this case.
    let c_str = serde_yaml::to_string(&c)?;
    let c_recover = serde_yaml::from_str(&c_str)?;
    assert_eq!(c, c_recover);
    Ok(())
}

#[derive(VersionedSerde, Clone, PartialEq, Debug)]
struct NewStruct {
    field: String,
}

#[test]
fn test_new_struct() -> Result<(), Box<dyn std::error::Error>> {
    let ns = NewStruct {
        field: "123".into(),
    };
    let ns_str = serde_yaml::to_string(&ns)?;
    let ns_recover = serde_yaml::from_str(&ns_str)?;
    assert_eq!(ns, ns_recover);

    Ok(())
}
