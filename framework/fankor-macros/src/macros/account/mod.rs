pub mod offset;
pub mod size;

use fankor_syn::fankor::read_fankor_toml;
use quote::quote;
use syn::spanned::Spanned;
use syn::{AttributeArgs, Error, Item};

use fankor_syn::Result;

pub fn processor(args: AttributeArgs, input: Item) -> Result<proc_macro::TokenStream> {
    // Process arguments.
    if !args.is_empty() {
        return Err(Error::new(
            input.span(),
            "account macro does not accept arguments",
        ));
    }

    // Read the Fankor.toml file.
    let config = read_fankor_toml();
    let accounts_config = config.accounts;

    // Process input.
    let (name, generics, item) = match &input {
        Item::Struct(item) => (&item.ident, &item.generics, quote! { #item }),
        Item::Enum(item) => (&item.ident, &item.generics, quote! { #item }),
        _ => {
            return Err(Error::new(
                input.span(),
                "account macro can only be applied to struct or enum declarations",
            ));
        }
    };

    let name_str = name.to_string();
    let generic_where_clause = &generics.where_clause;
    let generic_params = &generics.params;
    let generic_params = if generic_params.is_empty() {
        quote! {}
    } else {
        quote! { < #generic_params > }
    };

    let discriminator = accounts_config.get_discriminator(&name_str);

    let result = quote! {
        #[derive(FankorSerialize, FankorDeserialize)]
        #item

        #[automatically_derived]
        impl #generic_params ::fankor::traits::AccountSerialize for #name #generic_params #generic_where_clause {
            fn try_serialize<W: std::io::Write>(&self, writer: &mut W) -> ::fankor::errors::FankorResult<()> {
                if writer.write_all(<#name #generic_params as ::fankor::traits::Account>::discriminator()).is_err() {
                    return Err(::fankor::errors::FankorErrorCode::AccountDidNotSerialize{
                        account: #name_str.to_string()
                    }.into());
                }

                if ::fankor::prelude::borsh::BorshSerialize::serialize(self, writer).is_err() {
                    return Err(::fankor::errors::FankorErrorCode::AccountDidNotSerialize {
                        account: #name_str.to_string()
                    }.into());
                }
                Ok(())
            }
        }

        #[automatically_derived]
        impl #generic_params ::fankor::traits::AccountDeserialize for #name #generic_params #generic_where_clause {
            fn try_deserialize(buf: &mut &[u8]) -> ::fankor::errors::FankorResult<Self> {
                let discriminator = <#name #generic_params as ::fankor::traits::Account>::discriminator();
                let discriminator_len = discriminator.len();

                if buf.len() < discriminator_len {
                    return Err(::fankor::errors::FankorErrorCode::AccountDiscriminatorNotFound{
                        account: #name_str.to_string()
                    }.into());
                }

                let given_disc = &buf[..discriminator_len];
                if discriminator != given_disc {
                    return Err(::fankor::errors::FankorErrorCode::AccountDiscriminatorMismatch{
                        actual: given_disc.to_vec(),
                        expected: discriminator.to_vec(),
                        account: #name_str.to_string(),
                    }.into());
                }

                *buf = &buf[discriminator_len..];
                unsafe {Self::try_deserialize_unchecked(buf)}
            }

            unsafe fn try_deserialize_unchecked(buf: &mut &[u8]) -> ::fankor::errors::FankorResult<Self> {
                ::fankor::prelude::borsh::BorshDeserialize::deserialize(buf)
                    .map_err(|_| ::fankor::errors::FankorErrorCode::AccountDidNotDeserialize {
                    account: #name_str.to_string()
                }.into())
            }
        }

        #[automatically_derived]
        impl #generic_params ::fankor::traits::Account for #name #generic_params #generic_where_clause {
             fn discriminator() -> &'static [u8] {
                &[#(#discriminator,)*]
            }

             fn owner() -> &'static Pubkey {
                &crate::ID
            }
        }
    };

    Ok(result.into())
}
