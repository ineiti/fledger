use std::any::Any;

// Define a custom attribute macro for platform-specific async_trait
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::*;

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

            pub fn hash_data(data: &[u8]) -> Self {
                Self(U256::hash_data(data))
            }

            pub fn hash_domain_parts(domain: &str, parts: &[&[u8]]) -> Self {
                Self(U256::hash_domain_parts(domain, parts))
            }

            pub fn hash_domain_hashmap<K: Serialize, V: Serialize>(domain: &str, hm: &std::collections::HashMap<K, V>) -> Self {
                Self(U256::hash_domain_hashmap(domain, hm))
            }

            pub fn from_vec<V: Serialize>(domain: &str, v: &Vec<V>) -> Self {
                Self(U256::from_vec(domain, v))
            }

            pub fn to_bytes(&self) -> [u8; 32] {
                self.0.to_bytes()
            }

            pub fn len(&self) -> usize {
                32
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
            type Err = anyhow::Error;

            fn from_str(s: &str) -> anyhow::Result<#name> {
                Ok(Self(U256::from_str(s)?))
            }
        }

        impl AsRef<[u8]> for #name {
            fn as_ref(&self) -> &[u8] {
                &self.0.as_ref()
            }
        }

        impl From<[u8; 32]> for #name {
            fn from(b: [u8; 32]) -> Self {
                Self(b.into())
            }
        }

        impl From<U256> for #name {
            fn from(u: U256) -> Self {
                Self(u)
            }
        }

        impl std::fmt::LowerHex for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_fmt(format_args!("{:x}", self.0))?;
                Ok(())
            }
        }

        impl Into<U256> for #name {
            fn into(self) -> U256 {
                self.0.clone()
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(VersionedSerde, attributes(versions, serde))]
pub fn versioned_serde(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    // Extract the older configurations
    let versions: Vec<syn::Ident> = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("versions"))
        .and_then(|attr| match &attr.meta {
            Meta::NameValue(meta) => {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Str(lit), ..
                }) = &meta.value
                {
                    lit.parse::<syn::ExprArray>().ok().map(|list| {
                        list.elems
                            .into_iter()
                            .filter_map(|expr| {
                                if let syn::Expr::Path(expr_path) = expr {
                                    expr_path.path.get_ident().cloned()
                                } else {
                                    None
                                }
                            })
                            .collect()
                    })
                } else {
                    None
                }
            }
            _ => None,
        })
        .unwrap_or_default();

    let enum_name = format_ident!("{}Version", struct_name);
    let mut variants = vec![];
    let mut conversions = vec![];
    for (idx, version) in versions.iter().enumerate() {
        let version_number = idx + 1;
        let variant_name = format_ident!("{struct_name}V{}", version_number);
        variants.push(quote! {
            #variant_name(#version)
        });
        let next_variant_name = format_ident!("{struct_name}V{}", version_number + 1);
        conversions.push(quote! {
            #enum_name::#variant_name(#variant_name) => #enum_name::#next_variant_name(#variant_name.into()).into()
        });
    }

    // Add current variant to the enum and to the match conversion
    let mut latest_version = input.clone();
    let latest_version_name_m1 = format_ident!("{}V{}", struct_name, versions.len());
    let latest_version_name = format_ident!("{}V{}", struct_name, versions.len() + 1);
    latest_version.ident = latest_version_name.clone();
    // Don't copy 'versions' attribute, as it won't work recursively.
    latest_version
        .attrs
        .retain(|a| !a.path().is_ident("versions"));

    let m1_conversion = if versions.len() > 0 {
        quote! {
            impl From<#latest_version_name_m1> for #latest_version_name {
                fn from(value: #latest_version_name_m1) -> Self {
                    Into::<#struct_name>::into(value).into()
                }
            }
        }
    } else {
        quote! {}
    };

    let latest_enum = format_ident!("{struct_name}V{}", versions.len() + 1);
    variants.push(quote! {
        #latest_enum(#latest_version_name)
    });
    conversions.push(quote! {
        #enum_name::#latest_enum(orig) => orig.into()
    });

    let fields = match &input.data {
        syn::Data::Struct(struct_data) => match &struct_data.fields {
            syn::Fields::Named(fields_named) => &fields_named.named,
            _ => panic!("Only named fields are supported"),
        },
        _ => panic!("Only structs are supported"),
    };

    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();

    quote! {
        #[derive(serde::Serialize, serde::Deserialize, Clone)]
        enum #enum_name {
            #(#variants),*
        }

        #[derive(serde::Serialize, serde::Deserialize, Clone)]
        #latest_version

        #m1_conversion

        impl From<#struct_name> for #latest_version_name {
            fn from(orig: #struct_name) -> Self {
                Self {
                    #(#field_names: orig.#field_names),*
                }
            }
        }

        impl From<#latest_version_name> for #struct_name {
            fn from(new: #latest_version_name) -> Self {
                Self {
                    #(#field_names: new.#field_names),*
                }
            }
        }

        impl From<#enum_name> for #struct_name {
            fn from(value: #enum_name) -> #struct_name {
                return match value {
                    #(#conversions),*
                }
            }
        }

        impl serde::Serialize for #struct_name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                #enum_name::#latest_enum(self.clone().into()).serialize(serializer)
            }
        }

        impl<'de> serde::Deserialize<'de> for #struct_name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let version = #enum_name::deserialize(deserializer)?;
                Ok(version.into())
            }
        }
    }
    .into()
}

#[proc_macro_attribute]
pub fn target_send(_attr: TokenStream, input: TokenStream) -> TokenStream {
    if let Ok(input_trait) = syn::parse::<ItemTrait>(input.clone()) {
        let struct_name = &input_trait.ident;
        let box_name = format_ident!("{}Box", struct_name);
        let mut generics = input_trait.generics.clone();
        generics.type_params_mut().for_each(|param| {
            param.bounds.clear();
        });

        return quote! {
            #input_trait

            #[cfg(target_family = "wasm")]
            type #box_name #generics = Box<dyn #struct_name #generics>;

            #[cfg(target_family = "unix")]
            type #box_name #generics = Box<dyn #struct_name #generics + Send>;
        }
        .into();
    }

    if let Ok(input_type) = syn::parse::<ItemType>(input.clone()) {
        let mut type_as_string = quote! {#input_type}.to_string();
        type_as_string.insert_str(type_as_string.len() - 3, " + Send".into());
        let modified_type: syn::ItemType =
            syn::parse_str(&type_as_string).expect("Failed to parse modified type");

        return quote! {
            #[cfg(target_family = "unix")]
            #modified_type

            #[cfg(target_family = "wasm")]
            #input_type
        }
        .into();
    }

    let error = syn::Error::new(
        proc_macro2::Span::call_site(),
        format!("Unsupported type {:?}", input.type_id()),
    );
    TokenStream::from(error.to_compile_error())
}

use syn::{parse_macro_input, ItemFn, Lit, Meta};

#[proc_macro_attribute]
pub fn test_async_stack(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = if !attr.is_empty() {
        parse_macro_input!(attr with Punctuated::<Meta, Token![,]>::parse_terminated)
    } else {
        Punctuated::new()
    };
    let input = parse_macro_input!(item as ItemFn);

    // Ensure the function is async
    if input.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            input.sig.fn_token,
            "test_async_stack can only be applied to async functions",
        )
        .to_compile_error()
        .into();
    }

    // Default values
    let mut stack_size: usize = 10 * 1024 * 1024; // 10 MiB
    let mut worker_threads: usize = 8;

    // Parse attributes
    for meta in attrs {
        if let Meta::NameValue(nv) = meta {
            if nv.path.is_ident("stack_size") {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Int(lit_int),
                    ..
                }) = nv.value
                {
                    stack_size = lit_int.base10_parse().unwrap_or(stack_size);
                }
            } else if nv.path.is_ident("worker_threads") {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Int(lit_int),
                    ..
                }) = nv.value
                {
                    worker_threads = lit_int.base10_parse().unwrap_or(worker_threads);
                }
            }
        }
    }

    // Get the original function's parts
    let vis = input.vis;
    let sig = input.sig;
    let body = input.block;
    let attrs = input.attrs;
    let ident = sig.ident;

    // Generate the new test function with inlined run_big_stack functionality
    let output = quote! {
        #[test]
        #(#attrs)*
        #vis fn #ident() -> anyhow::Result<()> {
            std::thread::Builder::new()
                .stack_size(#stack_size)
                .spawn(move || {
                    tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(#worker_threads)
                        .enable_all()
                        .build()
                        .unwrap()
                        .block_on(async {
                            async #body.await
                        })
                })
                .unwrap()
                .join()
                .unwrap()
        }
    };

    output.into()
}
