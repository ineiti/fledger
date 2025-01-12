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

use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(AsU256)]
pub fn as_u256_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl #name {
            pub fn rnd() -> Self {
                Self(U256::random())
            }

            pub fn to_bytes(&self) -> [u8; 32] {
                self.0.to_be_bytes()
            }
        }

        impl std::fmt::Debug for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({:?})", stringify!(#name), self.0)
            }
        }

        impl std::ops::Deref for #name {
            type Target = U256;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl std::ops::DerefMut for #name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
    };

    TokenStream::from(expanded)
}