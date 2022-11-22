use crate::accounts::{ProgramAccount, ProgramAccountDiscriminant};
use fankor::prelude::*;

#[account(ProgramAccount)]
#[derive(AccountSize, AccountOffsets)]
pub enum EnumAccountData {
    A,
    B(u32, u64),
    C { value1: u32, value2: String },
}

#[account(ProgramAccount)]
#[derive(AccountSize, AccountOffsets, FankorZeroCopy)]
pub enum ZeroCopyEnumAccountData {
    A,
    B(u32, u64),
    C { value1: u32, value2: String },
}
