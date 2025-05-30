import * as anchor from "@coral-xyz/anchor";
import { Idl, AnchorProvider } from "@coral-xyz/anchor";
import { StakingContract } from "../target/types/staking_contract";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { assert } from "chai";

// Define account types based on the IDL
type StakingPoolAccount = {
    authority: PublicKey;
    rewardRate: anchor.BN;
    lockPeriod: anchor.BN;
    totalStaked: anchor.BN;
    lastUpdateTime: anchor.BN;
};

type StakingInfoAccount = {
    user: PublicKey;
    amount: anchor.BN;
    startTime: anchor.BN;
    lastClaimTime: anchor.BN;
};

describe("staking_contract", () => {
    // Configure the client to use the local cluster
    const provider = AnchorProvider.env();
    anchor.setProvider(provider);

    const program = anchor.workspace.StakingContract as anchor.Program<StakingContract>;

    // Test accounts
    const authority = anchor.web3.Keypair.generate();
    const user = anchor.web3.Keypair.generate();
    const mint = anchor.web3.Keypair.generate();

    // PDAs
    const [stakingPool] = PublicKey.findProgramAddressSync(
        [Buffer.from("staking_pool")],
        program.programId
    );

    const [userStakingInfo] = PublicKey.findProgramAddressSync(
        [Buffer.from("staking_info"), user.publicKey.toBuffer()],
        program.programId
    );

    // Token accounts
    let userTokenAccount: PublicKey;
    let stakingPoolTokenAccount: PublicKey;

    // Test parameters
    const rewardRate = new anchor.BN(1); // 1 token per second per staked token
    const lockPeriod = new anchor.BN(60); // 60 seconds lock period
    const stakeAmount = new anchor.BN(1000); // 1000 tokens to stake

    before(async () => {
        // Airdrop SOL to authority and user
        await provider.connection.requestAirdrop(authority.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
        await provider.connection.requestAirdrop(user.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);

        // Create token accounts
        userTokenAccount = await anchor.utils.token.associatedAddress({
            mint: mint.publicKey,
            owner: user.publicKey,
        });

        stakingPoolTokenAccount = await anchor.utils.token.associatedAddress({
            mint: mint.publicKey,
            owner: stakingPool,
        });
    });

    it("Initializes the staking pool", async () => {
        try {
            await program.methods
                .initializeStaking(rewardRate, lockPeriod)
                .accounts({
                    stakingPool,
                    authority: authority.publicKey,
                    systemProgram: SystemProgram.programId,
                })
                .signers([authority])
                .rpc();

            const stakingPoolAccount = await program.account.stakingPool.fetch(stakingPool);
            assert.ok(stakingPoolAccount.authority.equals(authority.publicKey));
            assert.ok(stakingPoolAccount.rewardRate.eq(rewardRate));
            assert.ok(stakingPoolAccount.lockPeriod.eq(lockPeriod));
            assert.ok(stakingPoolAccount.totalStaked.eq(new anchor.BN(0)));
        } catch (err) {
            console.error(err);
            throw err;
        }
    });

    it("Stakes tokens", async () => {
        try {
            await program.methods
                .stake(stakeAmount)
                .accounts({
                    stakingPool,
                    stakingInfo: userStakingInfo,
                    userTokenAccount,
                    stakingPoolTokenAccount,
                    user: user.publicKey,
                    tokenProgram: TOKEN_PROGRAM_ID,
                    systemProgram: SystemProgram.programId,
                })
                .signers([user])
                .rpc();

            const stakingInfoAccount = await program.account.stakingInfo.fetch(userStakingInfo);
            assert.ok(stakingInfoAccount.user.equals(user.publicKey));
            assert.ok(stakingInfoAccount.amount.eq(stakeAmount));

            const stakingPoolAccount = await program.account.stakingPool.fetch(stakingPool);
            assert.ok(stakingPoolAccount.totalStaked.eq(stakeAmount));
        } catch (err) {
            console.error(err);
            throw err;
        }
    });

    it("Claims rewards", async () => {
        try {
            // Wait for some time to accumulate rewards
            await new Promise(resolve => setTimeout(resolve, 2000)); // Wait 2 seconds

            await program.methods
                .claimRewards()
                .accounts({
                    stakingPool,
                    stakingInfo: userStakingInfo,
                    userTokenAccount,
                    stakingPoolTokenAccount,
                    user: user.publicKey,
                    tokenProgram: TOKEN_PROGRAM_ID,
                    systemProgram: SystemProgram.programId,
                })
                .signers([user])
                .rpc();

            const stakingInfoAccount = await program.account.stakingInfo.fetch(userStakingInfo);
            assert.ok(stakingInfoAccount.lastClaimTime.gt(new anchor.BN(0)));
        } catch (err) {
            console.error(err);
            throw err;
        }
    });

    it("Unstakes tokens after lock period", async () => {
        try {
            // Wait for lock period to end
            await new Promise(resolve => setTimeout(resolve, 61000)); // Wait 61 seconds

            await program.methods
                .unstake()
                .accounts({
                    stakingPool,
                    stakingInfo: userStakingInfo,
                    userTokenAccount,
                    stakingPoolTokenAccount,
                    user: user.publicKey,
                    tokenProgram: TOKEN_PROGRAM_ID,
                    systemProgram: SystemProgram.programId,
                })
                .signers([user])
                .rpc();

            // Verify staking info account is closed
            const stakingInfoAccount = await program.account.stakingInfo.fetch(userStakingInfo).catch(() => null);
            assert.isNull(stakingInfoAccount);

            // Verify total staked amount is updated
            const stakingPoolAccount = await program.account.stakingPool.fetch(stakingPool);
            assert.ok(stakingPoolAccount.totalStaked.eq(new anchor.BN(0)));
        } catch (err) {
            console.error(err);
            throw err;
        }
    });

    it("Fails to unstake before lock period", async () => {
        try {
            // Create new staking info for testing
            const newUser = anchor.web3.Keypair.generate();
            const [newStakingInfo] = PublicKey.findProgramAddressSync(
                [Buffer.from("staking_info"), newUser.publicKey.toBuffer()],
                program.programId
            );

            // Stake tokens
            await program.methods
                .stake(stakeAmount)
                .accounts({
                    stakingPool,
                    stakingInfo: newStakingInfo,
                    userTokenAccount,
                    stakingPoolTokenAccount,
                    user: newUser.publicKey,
                    tokenProgram: TOKEN_PROGRAM_ID,
                    systemProgram: SystemProgram.programId,
                })
                .signers([newUser])
                .rpc();

            // Try to unstake immediately
            await program.methods
                .unstake()
                .accounts({
                    stakingPool,
                    stakingInfo: newStakingInfo,
                    userTokenAccount,
                    stakingPoolTokenAccount,
                    user: newUser.publicKey,
                    tokenProgram: TOKEN_PROGRAM_ID,
                    systemProgram: SystemProgram.programId,
                })
                .signers([newUser])
                .rpc();

            assert.fail("Should have failed to unstake before lock period");
        } catch (err) {
            assert.include(err.message, "Lock period has not ended yet");
        }
    });
}); 