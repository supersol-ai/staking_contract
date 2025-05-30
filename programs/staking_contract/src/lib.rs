use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

declare_id!("CeJBBeP56NLJkGSoDzm6CQxJ2NXSo7jeEsXrhkZ3Mr55");

#[program]
pub mod staking_contract {
    use super::*;

    pub fn initialize_staking(
        ctx: Context<InitializeStaking>,
        reward_rate: u64,
        lock_period: i64,
    ) -> Result<()> {
        let staking_pool = &mut ctx.accounts.staking_pool;
        staking_pool.authority = ctx.accounts.authority.key();
        staking_pool.reward_rate = reward_rate;
        staking_pool.lock_period = lock_period;
        staking_pool.total_staked = 0;
        staking_pool.last_update_time = Clock::get()?.unix_timestamp;
        Ok(())
    }

    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        // Transfer tokens from user to staking pool
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.staking_pool_token_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );
        transfer(transfer_ctx, amount)?;

        // Update staking info
        let staking_info = &mut ctx.accounts.staking_info;
        staking_info.user = ctx.accounts.user.key();
        staking_info.amount = amount;
        staking_info.start_time = Clock::get()?.unix_timestamp;
        staking_info.last_claim_time = Clock::get()?.unix_timestamp;

        // Update total staked amount
        let staking_pool = &mut ctx.accounts.staking_pool;
        staking_pool.total_staked = staking_pool
            .total_staked
            .checked_add(amount)
            .ok_or(StakingError::ArithmeticOverflow)?;

        Ok(())
    }

    pub fn unstake(ctx: Context<Unstake>) -> Result<()> {
        let staking_info = &ctx.accounts.staking_info;
        let current_time = Clock::get()?.unix_timestamp;

        // Check if lock period has passed
        require!(
            current_time >= staking_info.start_time + ctx.accounts.staking_pool.lock_period,
            StakingError::LockPeriodNotOver
        );

        // Calculate rewards
        let rewards = calculate_rewards(
            staking_info.amount,
            staking_info.last_claim_time,
            current_time,
            ctx.accounts.staking_pool.reward_rate,
        )?;

        // Transfer rewards to user
        let rewards_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.staking_pool_token_account.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.staking_pool.to_account_info(),
            },
        );
        transfer(rewards_ctx, rewards)?;

        // Transfer staked tokens back to user
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.staking_pool_token_account.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.staking_pool.to_account_info(),
            },
        );
        transfer(transfer_ctx, staking_info.amount)?;

        // Update total staked amount
        let staking_pool = &mut ctx.accounts.staking_pool;
        staking_pool.total_staked = staking_pool
            .total_staked
            .checked_sub(staking_info.amount)
            .ok_or(StakingError::ArithmeticOverflow)?;

        // Close staking info account
        ctx.accounts
            .staking_info
            .close(ctx.accounts.user.to_account_info())?;

        Ok(())
    }

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        let staking_info = &mut ctx.accounts.staking_info;
        let current_time = Clock::get()?.unix_timestamp;

        // Calculate rewards
        let rewards = calculate_rewards(
            staking_info.amount,
            staking_info.last_claim_time,
            current_time,
            ctx.accounts.staking_pool.reward_rate,
        )?;

        // Update last claim time
        staking_info.last_claim_time = current_time;

        // Transfer rewards to user
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.staking_pool_token_account.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.staking_pool.to_account_info(),
            },
        );
        transfer(transfer_ctx, rewards)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeStaking<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + StakingPool::LEN,
        seeds = [b"staking_pool"],
        bump
    )]
    pub staking_pool: Account<'info, StakingPool>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub staking_pool: Account<'info, StakingPool>,

    #[account(
        init,
        payer = user,
        space = 8 + StakingInfo::LEN,
        seeds = [b"staking_info", user.key().as_ref()],
        bump
    )]
    pub staking_info: Account<'info, StakingInfo>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub staking_pool_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub staking_pool: Account<'info, StakingPool>,

    #[account(
        mut,
        close = user,
        seeds = [b"staking_info", user.key().as_ref()],
        bump
    )]
    pub staking_info: Account<'info, StakingInfo>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub staking_pool_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub staking_pool: Account<'info, StakingPool>,

    #[account(
        mut,
        seeds = [b"staking_info", user.key().as_ref()],
        bump
    )]
    pub staking_info: Account<'info, StakingInfo>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub staking_pool_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct StakingPool {
    pub authority: Pubkey,
    pub reward_rate: u64, // Rewards per token per second
    pub lock_period: i64, // Lock period in seconds
    pub total_staked: u64,
    pub last_update_time: i64,
}

impl StakingPool {
    pub const LEN: usize = 32 + 8 + 8 + 8 + 8;
}

#[account]
pub struct StakingInfo {
    pub user: Pubkey,
    pub amount: u64,
    pub start_time: i64,
    pub last_claim_time: i64,
}

impl StakingInfo {
    pub const LEN: usize = 32 + 8 + 8 + 8;
}

fn calculate_rewards(
    amount: u64,
    last_claim_time: i64,
    current_time: i64,
    reward_rate: u64,
) -> Result<u64> {
    let time_staked = current_time
        .checked_sub(last_claim_time)
        .ok_or(StakingError::ArithmeticOverflow)?;

    let rewards = amount
        .checked_mul(reward_rate)
        .ok_or(StakingError::ArithmeticOverflow)?
        .checked_mul(time_staked as u64)
        .ok_or(StakingError::ArithmeticOverflow)?;

    Ok(rewards)
}

#[error_code]
pub enum StakingError {
    #[msg("Lock period has not ended yet")]
    LockPeriodNotOver,
    #[msg("Arithmetic overflow occurred")]
    ArithmeticOverflow,
}
