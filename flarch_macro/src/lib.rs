// Define a custom attribute macro for platform-specific async_trait
use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemImpl, ItemTrait};

#[proc_macro_attribute]
pub fn platform_async_trait(_attr: TokenStream, input: TokenStream) -> TokenStream {
    if let Ok(input_impl) = syn::parse::<ItemImpl>(input.clone()) {
        let expanded = quote! {
            #[cfg_attr(target_family = "wasm", async_trait::async_trait(?Send))]
            #[cfg_attr(target_family = "unix", async_trait::async_trait)]
            #input_impl
        };
        return TokenStream::from(expanded);
    }

    if let Ok(input_trait) = syn::parse::<ItemTrait>(input) {
        let expanded = quote! {
            #[cfg_attr(target_family = "wasm", async_trait::async_trait(?Send))]
            #[cfg_attr(target_family = "unix", async_trait::async_trait)]
            #input_trait
        };
        return TokenStream::from(expanded);
    }

    let error = syn::Error::new(proc_macro2::Span::call_site(), "Unsupported type");
    TokenStream::from(error.to_compile_error())
}
