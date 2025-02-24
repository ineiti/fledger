# flmacro

Macros for the use in fledger.

## platrform_async_trait

This holds the macro for defining an `async_trait` either with or without the
`Send` trait.
You can use it like this:

```rust
#[platform_async_trait()]
impl SubsystemHandler<Message> for SomeBroker {
    async fn messages(&mut self, _: Vec<Message>) -> Vec<Message> {
        todo!();
    }
}
```

Depending on `wasm` or `unix`, it will either remove or keep the `Send` trait.

## proc_macro_derive(AsU256)

This allows to use tuple struct, or a newtype struct, based on `U256`, to export
all methods from `U256`.
Instead of using a type definition, which is not unique and can be replaced by any
of the other types, tuple structs allow for more type safety.
You can use it like this:

```rust
#[derive(AsU256)]
struct MyID(U256);
```

And now you can have `MyID::rnd()` and all the other methods from `U256`.

## proc_macro_derive(VersionedSerde, attributes(versions, serde))

To store configuration and other data in different version, you can use this 
derive macro:

```rust
use bytes::Bytes;
use flmacro::VersionedSerde;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

#[serde_as]
#[derive(VersionedSerde, Clone, PartialEq, Debug)]
#[versions = "[ConfigV1, ConfigV2]"]
struct Config {
    #[serde_as(as = "Base64")]
    name3: Bytes,
}

impl From<ConfigV2> for Config {
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
```

It will do the following:
- create a copy of the `struct Config` as `struct ConfigV3` with appropriate `FROM` implementations
- create a `ConfigVersion` enum with all configs in it
- implement `serde::Serialize` and `serde::Deserialize` on `Config` which will
  - wrap `Config` in the `ConfigVersion`
  - serialize the `ConfigVersion`, or
  - deserialize the `ConfigVersion` and convert it to `Config`

This allows you to use any serde implementation to store any version of your structure,
and retrieve always the latest version:

```rust
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
```

### Usage of serde_as and others

To allow usage of `serde_as`, the `VersionedSerde` also defines the `serde` attribute.
However, `VersionedSerde` does not use it itself.

### Usage of a new configuration structure

When you start with a new configuration structure, the `versions` can be omitted:

```rust
#[derive(VersionedSerde, Clone)]
struct NewStruct {
    field: String
}
```

When converting this using `serde`, it will store it as `V1`.
So whenever you create a new version, you can add it with
a converter to the latest structure:

```rust
#[derive(VersionedSerde, Clone)]
#[versions = "[NewStructV1]"]
struct NewStruct {
    field: String,
    other: String
}

impl From<NewStructV1> for NewStruct {
    fn from(value: NewStructV1) -> Self {
        Self {
            field: value.field,
            other: "default".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct NewStructV1 {
    field: String
}
```

## target_send

This macro does two things:
- for traits, it creates a version of the trait with `Box` at the end, and
it adds `+ Send` for non-wasm targets
- for types, it adds `+ Send` three characters from the end of the type in
rust-macro-format

So the following

```rust
#[target_send]
trait Something<T>{}

#[target_send]
type SomethingElse<T> = Box<dyn SomethingMore<T>>;
```

Will be translated to:

```rust
trait Something<T>{}

type SomethingBox<T> = Box<dyn Something<T> $SEND>

type SomethingElse<T> = Box<dyn SomethingMore<T> $SEND>;
```

with `$SEND` either an empty string for `wasm` targets, or `+ Send` for non-`wasm` targets.
Specifically the handling of the `type` is very ugly, as it converts the type to a string,
adds conditionally `+ Send` three characters from the end, and parses the resulting string.

Kids, don't do this at home...