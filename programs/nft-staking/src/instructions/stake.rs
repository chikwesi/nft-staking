use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    fetch_plugin,
    instructions::{
        AddPluginV1CpiBuilder, UpdateCollectionPluginV1CpiBuilder, UpdatePluginV1CpiBuilder,
    },
    types::{
        Attribute, Attributes, BurnDelegate, FreezeDelegate, Plugin, PluginAuthority, PluginType,
        UpdateAuthority,
    },
    ID as MPL_CORE_ID,
};
#[derive(Accounts)]

// stake update attribute, added stake and staked at
// freeze the asset
// update teh collection with a staked count.

pub struct Stake<'info> {
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
    pub system_program: Program<'info, System>,
    /// CHECK: This is the ID of the MPL Core Program
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
}

pub fn stake(ctx: Context<Stake>) -> Result<()> {
    let attributes_fetched: Option<Attributes> = fetch_plugin::<BaseAssetV1, Attributes>(
        &ctx.accounts.asset.to_account_info(),
        PluginType::Attributes,
    )
    .ok()
    .map(|(_, attributes, _)| attributes);

    let mut asset_attributes: Vec<Attribute> = Vec::new();

    if let Some(attribues) = &attributes_fetched {
        for attribute in &attribues.attribute_list {
            if attribute.key == "staked" {
                require!(attribute.value == "false", ErrorCode::AlreadyStaked);
            } else if attribute.key != "staked_at" {
                asset_attributes.push(attribute.clone());
            }
        }
    }

    let staked_attributes = vec![
        Attribute {
            key: "staked".to_string(),
            value: "true".to_string(),
        },
        Attribute {
            key: "staked_at".to_string(),
            value: Clock::get()?.unix_timestamp.to_string(),
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

    if attributes_fetched.is_none() {
        AddPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
            .asset(&ctx.accounts.asset.to_account_info())
            .authority(Some(&ctx.accounts.update_authority.to_account_info()))
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .payer(&ctx.accounts.user.to_account_info())
            .plugin(Plugin::Attributes(Attributes {
                attribute_list: asset_attributes,
            }))
            .system_program(&ctx.accounts.system_program)
            .init_authority(PluginAuthority::UpdateAuthority)
            .invoke_signed(&[signer_seeds])?;
    } else {
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
    }

    add_freeze_and_burn(&ctx)?;
    record_total_stake(&ctx)?;

    Ok(())
}

pub fn add_freeze_and_burn(ctx: &Context<Stake>) -> Result<()> {
    AddPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.user.to_account_info())
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: true }))
        .init_authority(PluginAuthority::UpdateAuthority)
        .system_program(&ctx.accounts.system_program)
        .invoke()?;

    AddPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.user.to_account_info())
        .plugin(Plugin::BurnDelegate(BurnDelegate {}))
        .init_authority(PluginAuthority::UpdateAuthority)
        .system_program(&ctx.accounts.system_program)
        .invoke()?;

    Ok(())
}

pub fn record_total_stake(ctx: &Context<Stake>) -> Result<()> {
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
                attribute.value = value.checked_add(1).unwrap_or(0).to_string();
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
