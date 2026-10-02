use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};
use mpl_core::{accounts::BaseCollectionV1, instructions::CreateV2CpiBuilder, ID as MPL_CORE_ID};
#[derive(Accounts)]
pub struct CreateMint<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub collection: Signer<'info>,
    #[account(mut)]
    pub asset: Signer<'info>,

    /// CHECK: for signing purpose
    #[account(
        seeds = [b"update_authority", collection.key().as_ref()],
        bump,
    )]
    pub update_authority: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    /// CHECK: This is the ID of the MPL Core Program
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
}

pub fn handle_mint(ctx: Context<CreateMint>, name: String, uri: String) -> Result<()> {
    let collection_key = ctx.accounts.collection.key();
    let signer_seeds = &[
        b"update_authority",
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.user.to_account_info())
        .owner(Some(&ctx.accounts.user.to_account_info()))
        .update_authority(None)
        .system_program(&ctx.accounts.system_program)
        .name(name)
        .uri(uri)
        .invoke_signed(&[signer_seeds])?;
    Ok(())
}
