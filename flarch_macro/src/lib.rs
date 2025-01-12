// Define a custom attribute macro for platform-specific async_trait
use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemImpl, ItemTrait, parse_macro_input, DeriveInput};

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

#[proc_macro_derive(AsU256)]
pub fn as_u256_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl #name {
            pub fn rnd() -> Self {
                Self(U256::rnd())
            }

            pub fn zero() -> Self {
                Self(U256::zero())
            }

            pub fn from_hash(domain: &str, parts: &[&[u8]]) -> Self {
                Self(U256::from_hash(domain, parts))
            }

            pub fn to_bytes(&self) -> [u8; 32] {
                self.0.to_bytes()
            }
        }

        impl std::fmt::Display for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_fmt(format_args!("{}", self.0))
            }
        }

        impl std::fmt::Debug for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_fmt(format_args!("{:?}", self.0))
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

        impl std::str::FromStr for #name {
            type Err = flarch::nodeids::ParseError;

            fn from_str(s: &str) -> Result<#name, flarch::nodeids::ParseError> {
                Ok(Self(U256::from_str(s)?))
            }
        }

        impl AsRef<[u8]> for #name {
            fn as_ref(&self) -> &[u8] {
                &self.0.as_ref()
            }
        }
        impl From<sha2::digest::generic_array::GenericArray<u8, sha2::digest::consts::U32>> for #name {
            fn from(ga: sha2::digest::generic_array::GenericArray<u8, sha2::digest::consts::U32>) -> Self {
                Self(ga.into())
            }
        }

        impl From<[u8; 32]> for #name {
            fn from(b: [u8; 32]) -> Self {
                Self(b.into())
            }
        }

        impl std::fmt::LowerHex for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_fmt(format_args!("{:x}", self.0))?;
                Ok(())
            }
        }
    };

    TokenStream::from(expanded)
}
