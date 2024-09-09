# flarch_macro

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