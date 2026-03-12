use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, TokenInterface, TokenAccount, Mint, TransferChecked};

declare_id!("FJ22iEN7eM9XqZRKLNR1xiraTm1pZNaN56RRvM9M73oy");

#[program]
pub mod token_lock_staking {
    use super::*;

    pub fn create_lock_vault(ctx: Context<CreateLockVault>, unlock_at: i64, amount: u64, stake_id: String) -> Result<()> {
        require!(unlock_at > Clock::get()?.unix_timestamp, ErrorCode::InvalidUnlockTime);
        require!(amount > 0, ErrorCode::InvalidAmount);
        require!(stake_id.len() <= 64, ErrorCode::StakeIdTooLong);
        let vault = &mut ctx.accounts.vault;
        vault.owner = ctx.accounts.user.key();
        vault.mint = ctx.accounts.mint.key();
        vault.token_program = ctx.accounts.token_program.key();
        vault.unlock_at = unlock_at;
        vault.amount = amount;
        vault.stake_id = stake_id;
        vault.created_at = Clock::get()?.unix_timestamp;
        vault.locked = true;
        vault.claimed = false;
        vault.tokens_deposited = false;
        Ok(())
    }

    pub fn lock_tokens(ctx: Context<LockTokens>, amount: u64) -> Result<()> {
        let vault = &ctx.accounts.vault;
        require!(vault.locked, ErrorCode::VaultUnlocked);
        require!(vault.amount == amount, ErrorCode::AmountMismatch);
        require!(!vault.claimed, ErrorCode::AlreadyClaimed);
        require!(!vault.tokens_deposited, ErrorCode::AlreadyDeposited);
        require!(vault.owner == ctx.accounts.user.key(), ErrorCode::Unauthorized);
        let decimals = ctx.accounts.mint.decimals;
        token_interface::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.vault_token_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                },
            ),
            amount,
            decimals,
        )?;
        ctx.accounts.vault.tokens_deposited = true;
        Ok(())
    }

    pub fn claim_tokens(ctx: Context<ClaimTokens>) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        let clock = Clock::get()?;
        require!(!vault.claimed, ErrorCode::AlreadyClaimed);
        require!(vault.locked, ErrorCode::VaultNotLocked);
        require!(vault.tokens_deposited, ErrorCode::NothingDeposited);
        require!(clock.unix_timestamp >= vault.unlock_at, ErrorCode::StillLocked);
        let bump = ctx.bumps.vault;
        let owner_key = ctx.accounts.user.key();
        let stake_id = vault.stake_id.clone();
        let signer_seeds: &[&[&[u8]]] = &[&[b"lock_vault", owner_key.as_ref(), stake_id.as_bytes(), &[bump]]];
        let decimals = ctx.accounts.mint.decimals;
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.vault_token_account.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: vault.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                },
                signer_seeds,
            ),
            vault.amount,
            decimals,
        )?;
        vault.claimed = true;
        vault.locked = false;
        vault.claimed_at = Some(clock.unix_timestamp);
        Ok(())
    }
}

#[account]
pub struct LockVault {
    pub owner: Pubkey,
    pub mint: Pubkey,
    pub token_program: Pubkey,
    pub unlock_at: i64,
    pub amount: u64,
    pub stake_id: String,
    pub created_at: i64,
    pub claimed: bool,
    pub claimed_at: Option<i64>,
    pub locked: bool,
    pub tokens_deposited: bool,
}

#[derive(Accounts)]
#[instruction(unlock_at: i64, amount: u64, stake_id: String)]
pub struct CreateLockVault<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        init, payer = user,
        space = 8 + 32 + 32 + 32 + 8 + 8 + (4 + 64) + 8 + 1 + 9 + 1 + 1,
        seeds = [b"lock_vault", user.key().as_ref(), stake_id.as_bytes()], bump
    )]
    pub vault: Account<'info, LockVault>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct LockTokens<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut,
        constraint = vault.owner == user.key() @ ErrorCode::Unauthorized,
        constraint = token_program.key() == vault.token_program @ ErrorCode::TokenProgramMismatch
    )]
    pub vault: Account<'info, LockVault>,
    #[account(mut)]
    pub user_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init_if_needed, payer = user,
        associated_token::mint = mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
}

#[derive(Accounts)]
pub struct ClaimTokens<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut,
        seeds = [b"lock_vault", user.key().as_ref(), vault.stake_id.as_bytes()], bump,
        constraint = vault.owner == user.key() @ ErrorCode::Unauthorized,
        constraint = token_program.key() == vault.token_program @ ErrorCode::TokenProgramMismatch
    )]
    pub vault: Account<'info, LockVault>,
    #[account(mut)]
    pub user_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut,
        associated_token::mint = mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Unlock time must be in the future")] InvalidUnlockTime,
    #[msg("Amount must be greater than 0")] InvalidAmount,
    #[msg("Stake ID too long (max 64)")] StakeIdTooLong,
    #[msg("Vault is unlocked")] VaultUnlocked,
    #[msg("Amount mismatch")] AmountMismatch,
    #[msg("Already claimed")] AlreadyClaimed,
    #[msg("Vault not locked")] VaultNotLocked,
    #[msg("Still locked")] StillLocked,
    #[msg("Unauthorized")] Unauthorized,
    #[msg("Token program mismatch")] TokenProgramMismatch,
    #[msg("Tokens already deposited")] AlreadyDeposited,
    #[msg("No tokens deposited to claim")] NothingDeposited,
}
