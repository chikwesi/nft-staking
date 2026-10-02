use crate::{error::ErrorCode, Config};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{mint_to_checked, Mint, MintToChecked, TokenAccount, TokenInterface},
};
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    fetch_plugin,
    instructions::{
        AddPluginV1CpiBuilder, UpdateCollectionPluginV1CpiBuilder, UpdatePluginV1CpiBuilder,
    },
    types::{
        Attribute, Attributes, FreezeDelegate, Plugin, PluginAuthority, PluginType, UpdateAuthority,
    },
    ID as MPL_CORE_ID,
};

const SECONDS_PER_DAY: i64 = 86400;

// stake update attribute, added stake and staked at
// freeze the asset
// update teh collection with a staked count.

#[derive(Accounts)]
pub struct UnStake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut,
        has_one = update_authority @ ErrorCode::InvalidUpdateAuthority)]
    pub collection: Account<'info, BaseCollectionV1>,
    #[account(mut,
        owner = user.key() @ ErrorCode::InvalidOwner,
        constraint = asset.update_authority == UpdateAuthority::Collection(collection.key()) @ ErrorCode::InvalidUpdateAuthority
        )]
    pub asset: Account<'info, BaseAssetV1>,
    /// CHECK: for signing purpose
    #[account(
        seeds = [b"update_authority", collection.key().as_ref()],
        bump,
    )]
    pub update_authority: UncheckedAccount<'info>,
    #[account(
        seeds = [b"config", collection.key().as_ref()],
        bump = config.bump
        )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [b"rewards_mint", config.key().as_ref()],
        bump = config.rewards_bump
        )]
    pub reward_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = reward_mint,
        associated_token::authority= user
    )]
    pub user_ata: InterfaceAccount<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: This is the ID of the MPL Core Program
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
}

pub fn unstake(ctx: Context<UnStake>) -> Result<()> {
    let attributes_fetched: Option<Attributes> = fetch_plugin::<BaseAssetV1, Attributes>(
        &ctx.accounts.asset.to_account_info(),
        PluginType::Attributes,
    )
    .ok()
    .map(|(_, attributes, _)| attributes);

    let current_timestamp = Clock::get()?.unix_timestamp;
    let mut asset_attributes: Vec<Attribute> = Vec::new();
    let mut staked_time: u64 = 0;
    let mut has_staked = (false, false);
    if let Some(attribues) = &attributes_fetched {
        for attribute in &attribues.attribute_list {
            if attribute.key == "staked" {
                has_staked.0 = true;
                require!(attribute.value == "true", ErrorCode::AssetNotStaked);
            } else if attribute.key == "staked_at" {
                has_staked.1 = true;
                staked_time = current_timestamp
                    .checked_sub(
                        attribute
                            .value
                            .parse::<i64>()
                            .map_err(|_| ErrorCode::InvalidTimestamp)?,
                    )
                    .ok_or(ErrorCode::InvalidTimestamp)?
                    .checked_div(SECONDS_PER_DAY) //staked time in days
                    .ok_or(ErrorCode::InvalidTimestamp)? as u64;
                require!(
                    staked_time >= ctx.accounts.config.freeze_period_in_days as u64,
                    ErrorCode::FreezePeriodNotElapsed
                );
            } else {
                asset_attributes.push(attribute.clone());
            }
        }
    }

    require!(has_staked.1 && has_staked.0, ErrorCode::AssetNotStaked);

    let staked_attributes = vec![
        Attribute {
            key: "staked".to_string(),
            value: "false".to_string(),
        },
        Attribute {
            key: "staked_at".to_string(),
            value: "0".to_string(),
        },
    ];

    for attribute in staked_attributes {
        asset_attributes.push(attribute);
    }

    let collection_key = ctx.accounts.collection.key();
    let signer_seeds = &[
        b"update_authority",
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .plugin(Plugin::Attributes(Attributes {
            attribute_list: asset_attributes,
        }))
        .payer(&ctx.accounts.user.to_account_info())
        .system_program(&ctx.accounts.system_program)
        .invoke_signed(&[signer_seeds])?;

    unfreeze_asset(&ctx)?;
    update_total_stake(&ctx)?;
    mint_rewards(&ctx, staked_time)?;

    Ok(())
}

pub fn unfreeze_asset(ctx: &Context<UnStake>) -> Result<()> {
    let collection_key = ctx.accounts.collection.key();
    let signer_seeds = &[
        b"update_authority",
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];
    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.user.to_account_info())
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .system_program(&ctx.accounts.system_program)
        .invoke_signed(&[signer_seeds])?;

    Ok(())
}

pub fn mint_rewards(ctx: &Context<UnStake>, staked_time: u64) -> Result<()> {
    let amount: u64 = staked_time
        .checked_mul(ctx.accounts.config.rewards_bps as u64)
        .ok_or(ErrorCode::InvalidRewardsBps)?
        .checked_mul(10u64.pow(ctx.accounts.reward_mint.decimals as u32))
        .ok_or(ErrorCode::InvalidRewardsBps)?
        .checked_div(10_000u64)
        .ok_or(ErrorCode::InvalidRewardsBps)?;

    let collection_key = ctx.accounts.collection.key();
    let signer_seeds: &[&[&[u8]]] = &[&[
        b"config",
        collection_key.as_ref(),
        &[ctx.accounts.config.bump],
    ]];

    let mint_to_accounts = MintToChecked {
        mint: ctx.accounts.reward_mint.to_account_info(),
        to: ctx.accounts.user_ata.to_account_info(),
        authority: ctx.accounts.config.to_account_info(),
    };

    let cpi = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        mint_to_accounts,
        signer_seeds,
    );

    mint_to_checked(cpi, amount, ctx.accounts.reward_mint.decimals)
}

pub fn update_total_stake(ctx: &Context<UnStake>) -> Result<()> {
    let mut attributes_fetched: Option<Attributes> = fetch_plugin::<BaseCollectionV1, Attributes>(
        &ctx.accounts.collection.to_account_info(),
        PluginType::Attributes,
    )
    .ok()
    .map(|(_, attributes, _)| attributes);

    let mut attribute_list: Vec<Attribute> = Vec::new();

    if let Some(attribues) = &mut attributes_fetched {
        for attribute in &mut attribues.attribute_list {
            if attribute.key == "total_staked" {
                let value = attribute.value.parse::<u16>().unwrap_or(0);
                attribute.value = value.checked_sub(1).unwrap_or(0).to_string();
            }
            attribute_list.push(attribute.clone());
        }
    }
    let collection_key = ctx.accounts.collection.key();
    let signer_seeds = &[
        b"update_authority",
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    UpdateCollectionPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .collection(&ctx.accounts.collection.to_account_info())
        .plugin(Plugin::Attributes(Attributes { attribute_list }))
        .payer(&ctx.accounts.user.to_account_info())
        .system_program(&ctx.accounts.system_program)
        .invoke_signed(&[signer_seeds])?;

    Ok(())
}
