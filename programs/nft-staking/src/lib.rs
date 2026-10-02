pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("C85k4Vyqi67Bj9aCfAm5Vaca2HyqRrNdPR8eWVVzDaPF");

#[program]
pub mod nft_staking {

    use super::*;

    // pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
    //     crate::instructions::initialize::handle_initialize(ctx)
    // }

    pub fn initialize(
        ctx: Context<Initialize>,
        rewards_bps: u16,
        freeze_period: u16,
    ) -> Result<()> {
        instructions::handle_initialize(ctx, rewards_bps, freeze_period)
    }

    pub fn create_collection(
        ctx: Context<CreateCollection>,
        name: String,
        uri: String,
    ) -> Result<()> {
        instructions::handle_create(ctx, name, uri)
    }

    pub fn create_mint(ctx: Context<CreateMint>, name: String, uri: String) -> Result<()> {
        instructions::handle_mint(ctx, name, uri)
    }

    pub fn stake(ctx: Context<Stake>) -> Result<()> {
        instructions::stake(ctx)
    }

    pub fn un_stake(ctx: Context<UnStake>) -> Result<()> {
        instructions::unstake(ctx)
    }

    pub fn burn(ctx: Context<Burn>) -> Result<()> {
        instructions::burn_asset(ctx)
    }

    pub fn claim(ctx: Context<ClaimRewards>) -> Result<()> {
        instructions::claim(ctx)
    }
}
