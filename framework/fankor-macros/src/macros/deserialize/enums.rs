use crate::fnk_syn::FnkMetaArgumentList;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::spanned::Spanned;
use syn::{Attribute, Error, Fields, Ident, ItemEnum, Meta, NestedMeta, Path, WhereClause};

pub fn enum_de(input: &ItemEnum, crate_name: Ident) -> syn::Result<TokenStream2> {
    let name = &input.ident;

    // Check for fankor attribute.
    let mut account_discriminants = None;
    let mut is_accounts = false;

    for attr in &input.attrs {
        if attr.path.is_ident("fankor") {
            if let Ok(mut args) = attr.parse_args::<FnkMetaArgumentList>() {
                args.error_on_duplicated()?;

                if let Some(v) = args.pop_ident("account", true)? {
                    if is_accounts {
                        return Err(Error::new(
                            attr.span(),
                            "Cannot define both fankor::accounts and fankor::account attributes",
                        ));
                    }

                    account_discriminants = Some(v);
                }

                if args.pop_plain("accounts", true)? {
                    if account_discriminants.is_some() {
                        return Err(Error::new(
                            attr.span(),
                            "Cannot define both fankor::accounts and fankor::account attributes",
                        ));
                    }

                    is_accounts = true;
                }

                args.error_on_unknown()?;
            } else {
                return Err(Error::new(
                    attr.span(),
                    "The correct pattern is #[fankor(<meta_list>)]",
                ));
            };
            break;
        }
    }

    let account_discriminants = if let Some(account_discriminants) = account_discriminants {
        let message = format!("Invalid discriminant for enum variant {}", name);
        quote! {
            let discriminant:u8 = #crate_name::BorshDeserialize::deserialize(buf)?;
            if discriminant != #account_discriminants::#name.code() {
                return Err(
                    std::io::Error::new(std::io::ErrorKind::Other, #message)
                );
            }
        }
    } else {
        quote! {}
    };

    let discriminant_name = format_ident!("{}Discriminant", name);
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let mut where_clause = where_clause.map_or_else(
        || WhereClause {
            where_token: Default::default(),
            predicates: Default::default(),
        },
        Clone::clone,
    );
    let init_method = contains_initialize_with(&input.attrs)?;
    let mut variant_arms = TokenStream2::new();
    let mut variant_consts = TokenStream2::new();

    for variant in input.variants.iter() {
        let variant_ident = &variant.ident;
        let mut variant_header = TokenStream2::new();

        let const_name = format_ident!("{}Discriminant", variant_ident);

        variant_consts.extend(quote! {
            const #const_name: u8 = #discriminant_name::#variant_ident.code();
        });

        match &variant.fields {
            Fields::Named(fields) => {
                for field in &fields.named {
                    let field_name = field.ident.as_ref().unwrap();
                    if contains_skip(&field.attrs) {
                        variant_header.extend(quote! {
                            #field_name: Default::default(),
                        });
                    } else {
                        let field_type = &field.ty;
                        where_clause.predicates.push(
                            syn::parse2(quote! {
                                #field_type: #crate_name::BorshDeserialize
                            })
                            .unwrap(),
                        );

                        variant_header.extend(quote! {
                            #field_name: #crate_name::BorshDeserialize::deserialize(buf)?,
                        });
                    }
                }
                variant_header = quote! { { #variant_header }};
            }
            Fields::Unnamed(fields) => {
                for field in fields.unnamed.iter() {
                    if contains_skip(&field.attrs) {
                        variant_header.extend(quote! { Default::default(), });
                    } else {
                        let field_type = &field.ty;
                        where_clause.predicates.push(
                            syn::parse2(quote! {
                                #field_type: #crate_name::BorshDeserialize
                            })
                            .unwrap(),
                        );

                        variant_header
                            .extend(quote! { #crate_name::BorshDeserialize::deserialize(buf)?, });
                    }
                }
                variant_header = quote! { ( #variant_header )};
            }
            Fields::Unit => {}
        }

        variant_arms.extend(quote! {
            #const_name => #name::#variant_ident #variant_header ,
        });
    }

    let variant_reader = if is_accounts {
        quote! {
            let variant_idx: u8 = {
                let mut aux_buf = *buf;
                #crate_name::BorshDeserialize::deserialize(&mut aux_buf)?
            };
        }
    } else {
        quote! {
            let variant_idx: u8 = #crate_name::BorshDeserialize::deserialize(buf)?;
        }
    };

    if let Some(method_ident) = init_method {
        Ok(quote! {
            #[automatically_derived]
            impl #impl_generics #crate_name::de::BorshDeserialize for #name #ty_generics #where_clause {
                fn deserialize(buf: &mut &[u8]) -> core::result::Result<Self, #crate_name::maybestd::io::Error> {
                    #account_discriminants

                    #variant_consts
                    #variant_reader
                    let mut return_value = match variant_idx {
                        #variant_arms
                        _ => {
                            let msg = #crate_name::maybestd::format!("Unexpected variant index: {:?}", variant_idx);

                            return Err(#crate_name::maybestd::io::Error::new(
                                #crate_name::maybestd::io::ErrorKind::InvalidInput,
                                msg,
                            ));
                        }
                    };
                    return_value.#method_ident();
                    Ok(return_value)
                }
            }
        })
    } else {
        Ok(quote! {
            #[automatically_derived]
            #[allow(non_upper_case_globals)]
            impl #impl_generics #crate_name::de::BorshDeserialize for #name #ty_generics #where_clause {
                fn deserialize(buf: &mut &[u8]) -> core::result::Result<Self, #crate_name::maybestd::io::Error> {
                    #account_discriminants

                    #variant_consts
                    #variant_reader
                    let return_value = match variant_idx {
                        #variant_arms
                        _ => {
                            let msg = #crate_name::maybestd::format!("Unexpected variant index: {:?}", variant_idx);

                            return Err(#crate_name::maybestd::io::Error::new(
                                #crate_name::maybestd::io::ErrorKind::InvalidInput,
                                msg,
                            ));
                        }
                    };
                    Ok(return_value)
                }
            }
        })
    }
}

pub fn contains_skip(attrs: &[Attribute]) -> bool {
    for attr in attrs.iter() {
        if let Ok(Meta::Path(path)) = attr.parse_meta() {
            if path.to_token_stream().to_string().as_str() == "borsh_skip" {
                return true;
            }
        }
    }
    false
}

pub fn contains_initialize_with(attrs: &[Attribute]) -> syn::Result<Option<Path>> {
    for attr in attrs.iter() {
        if let Ok(Meta::List(meta_list)) = attr.parse_meta() {
            if meta_list.path.to_token_stream().to_string().as_str() == "borsh_init" {
                if meta_list.nested.len() != 1 {
                    return Err(Error::new(
                        meta_list.span(),
                        "borsh_init requires exactly one initialization method.",
                    ));
                }
                let nested_meta = meta_list.nested.iter().next().unwrap();
                if let NestedMeta::Meta(Meta::Path(path)) = nested_meta {
                    return Ok(Some(path.clone()));
                }
            }
        }
    }
    Ok(None)
}
