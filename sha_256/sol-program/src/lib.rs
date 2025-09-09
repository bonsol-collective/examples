use bonsol_interface::callback::{handle_callback, BonsolCallback};
use bonsol_interface::instructions::{execute_v1, CallbackConfig, ExecutionConfig, InputRef};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint,
    entrypoint::ProgramResult,
    instruction::AccountMeta,
    msg,
    program::invoke,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::Sysvar,
};

// Program ID
solana_program::declare_id!("72bGikYM7J314fvAfBDvMGdqaewHaq7LpbJMNF5rJDb8");

// Constants - using same image ID as sample for SHA256 hashing
const SHA256_IMAGE_ID: &str =
    "75029efa53432a9030e5e76d58fb34dfa786cd0f6182ed0741d635ff5e4f0341";
const PRIVATE_DATA_URL: &[u8] = b"https://echoserver.dev/server?response=N4IgFgpghgJhBOBnEAuA2mkBjA9gOwBcJCBaAgTwAcIQAaEIgDwIHpKAbKASzxAF0+9AEY4Y5VKArVUDCMzogYUAlBlFEBEAF96G5QFdkKAEwAGU1qA";

// Data structures
#[derive(Debug, Clone)]
pub struct EscrowAccount {
    pub seeds: [u8; 32],           // Store the seed used to derive this account
    pub amount_lamports: u64,      // Amount to be released to receiver
    pub hash: [u8; 64],            // SHA256 hex string as bytes (64 chars = 64 bytes)
    pub is_claimed: bool,          // Whether the escrow has been claimed
    pub receiver: Option<Pubkey>,  // The receiver (set when claimed)
    pub initializer: Pubkey,       // The account that initialized the escrow
}

impl EscrowAccount {
    pub const SIZE: usize = 32 + 8 + 64 + 1 + 1 + 32 + 32; // seeds + amount + hash + is_claimed + option_flag + receiver + initializer

    pub fn pack(&self, dst: &mut [u8]) -> ProgramResult {
        if dst.len() < Self::SIZE {
            return Err(ProgramError::AccountDataTooSmall);
        }

        dst[0..32].copy_from_slice(&self.seeds);
        dst[32..40].copy_from_slice(&self.amount_lamports.to_le_bytes());
        dst[40..104].copy_from_slice(&self.hash);
        dst[104] = if self.is_claimed { 1 } else { 0 };
        
        // Pack Option<Pubkey>
        match self.receiver {
            Some(receiver) => {
                dst[105] = 1; // Some flag
                dst[106..138].copy_from_slice(&receiver.to_bytes());
            }
            None => {
                dst[105] = 0; // None flag
                dst[106..138].fill(0);
            }
        }
        
        dst[138..170].copy_from_slice(&self.initializer.to_bytes());

        Ok(())
    }

    pub fn unpack(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() < Self::SIZE {
            return Err(ProgramError::AccountDataTooSmall);
        }

        let mut seeds = [0u8; 32];
        seeds.copy_from_slice(&src[0..32]);

        let amount_lamports = u64::from_le_bytes([
            src[32], src[33], src[34], src[35], src[36], src[37], src[38], src[39],
        ]);

        let mut hash = [0u8; 64];
        hash.copy_from_slice(&src[40..104]);

        let is_claimed = src[104] != 0;

        let receiver = if src[105] != 0 {
            Some(Pubkey::new_from_array([
                src[106], src[107], src[108], src[109], src[110], src[111], src[112], src[113],
                src[114], src[115], src[116], src[117], src[118], src[119], src[120], src[121],
                src[122], src[123], src[124], src[125], src[126], src[127], src[128], src[129],
                src[130], src[131], src[132], src[133], src[134], src[135], src[136], src[137],
            ]))
        } else {
            None
        };

        let initializer = Pubkey::new_from_array([
            src[138], src[139], src[140], src[141], src[142], src[143], src[144], src[145],
            src[146], src[147], src[148], src[149], src[150], src[151], src[152], src[153],
            src[154], src[155], src[156], src[157], src[158], src[159], src[160], src[161],
            src[162], src[163], src[164], src[165], src[166], src[167], src[168], src[169],
        ]);

        Ok(Self {
            seeds,
            amount_lamports,
            hash,
            is_claimed,
            receiver,
            initializer,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionTracker {
    pub execution_account: Pubkey,
}

impl ExecutionTracker {
    pub const SIZE: usize = 32;

    pub fn pack(&self, dst: &mut [u8]) -> ProgramResult {
        if dst.len() < Self::SIZE {
            return Err(ProgramError::AccountDataTooSmall);
        }
        dst[0..32].copy_from_slice(&self.execution_account.to_bytes());
        Ok(())
    }

    pub fn unpack(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() < Self::SIZE {
            return Err(ProgramError::AccountDataTooSmall);
        }
        let execution_account = Pubkey::new_from_array([
            src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7], src[8], src[9],
            src[10], src[11], src[12], src[13], src[14], src[15], src[16], src[17], src[18],
            src[19], src[20], src[21], src[22], src[23], src[24], src[25], src[26], src[27],
            src[28], src[29], src[30], src[31],
        ]);
        Ok(Self { execution_account })
    }
}

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    let (instruction, data) = instruction_data.split_first().unwrap();

    match instruction {
        0 => initialize_escrow(program_id, accounts, data),
        1 => claim_escrow(program_id, accounts, data),
        2 => handle_claim_callback(program_id, accounts, data),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

// Instruction 0: Initialize escrow
pub fn initialize_escrow(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // Parse instruction data: seed_len(1) + seed + hash_len(1) + hash + amount_lamports(8)
    if data.len() < 2 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let seed_len = data[0] as usize;
    if data.len() < 1 + seed_len + 1 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let seed = &data[1..1 + seed_len];
    let hash_len = data[1 + seed_len] as usize;
    
    if data.len() < 1 + seed_len + 1 + hash_len + 8 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let hash_str = &data[2 + seed_len..2 + seed_len + hash_len];
    let amount_lamports = u64::from_le_bytes(
        data[2 + seed_len + hash_len..2 + seed_len + hash_len + 8]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?
    );

    // Validate hash is exactly 64 hex characters
    if hash_len != 64 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let account_iter = &mut accounts.iter();
    let initializer = next_account_info(account_iter)?;
    let escrow_account = next_account_info(account_iter)?;
    let system_program = next_account_info(account_iter)?;

    if !initializer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Derive PDA for escrow account
    let (expected_pda, bump) = Pubkey::find_program_address(&[seed], program_id);
    if escrow_account.key != &expected_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    // Create account if it doesn't exist
    if escrow_account.lamports() == 0 {
        let space = EscrowAccount::SIZE + 100;
        let rent = Rent::get()?;
        let rent_exempt_lamports = rent.minimum_balance(space);
        let total_lamports = rent_exempt_lamports + amount_lamports;

        let create_account_ix = system_instruction::create_account(
            initializer.key,
            escrow_account.key,
            total_lamports,
            space as u64,
            program_id,
        );

        invoke_signed(
            &create_account_ix,
            &[
                initializer.clone(),
                escrow_account.clone(),
                system_program.clone(),
            ],
            &[&[seed, &[bump]]],
        )?;
    } else {
        // Transfer additional lamports to existing account
        let transfer_ix = system_instruction::transfer(
            initializer.key,
            escrow_account.key,
            amount_lamports,
        );

        invoke(
            &transfer_ix,
            &[initializer.clone(), escrow_account.clone()],
        )?;
    }

    // Initialize escrow account data
    let mut escrow_data = escrow_account.try_borrow_mut_data()?;
    let mut seeds_array = [0u8; 32];
    let copy_len = std::cmp::min(seed.len(), 32);
    seeds_array[..copy_len].copy_from_slice(&seed[..copy_len]);

    let mut hash_array = [0u8; 64];
    hash_array.copy_from_slice(hash_str);

    let escrow = EscrowAccount {
        seeds: seeds_array,
        amount_lamports,
        hash: hash_array,
        is_claimed: false,
        receiver: None,
        initializer: *initializer.key,
    };
    escrow.pack(&mut escrow_data)?;

    msg!("Escrow initialized with,lamports: {:?}, seed: {:?}, hash: {:?}, initializer: {:?}", amount_lamports, seed, hash_str, initializer.key);
    Ok(())
}

// Instruction 1: Claim escrow (triggers Bonsol execution)
pub fn claim_escrow(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    // Parse instruction data: execution_id(16) + bump(1) + tip(8) + expiry_offset(8) + seed_len(1) + seed + preimage_len(2) + preimage
    if data.len() < 35 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let execution_id = std::str::from_utf8(&data[0..16])
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let bump = data[16];
    let tip = u64::from_le_bytes(
        data[17..25]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?
    );
    let expiry_offset = u64::from_le_bytes(
        data[25..33]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?
    );
    
    let seed_len = data[33] as usize;
    if data.len() < 34 + seed_len + 2 {
        return Err(ProgramError::InvalidInstructionData);
    }
    
    let seed = &data[34..34 + seed_len];
    let preimage_len = u16::from_le_bytes(
        data[34 + seed_len..36 + seed_len]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?
    ) as usize;
    
    if data.len() < 36 + seed_len + preimage_len {
        return Err(ProgramError::InvalidInstructionData);
    }
    
    let preimage = &data[36 + seed_len..36 + seed_len + preimage_len];
    let preimageStr = std::str::from_utf8(&preimage[..]).unwrap();
    msg!("Preimage to hash: {}", preimageStr);

    let account_iter = &mut accounts.iter();
    let payer = next_account_info(account_iter)?;
    let receiver = next_account_info(account_iter)?;
    let escrow_account = next_account_info(account_iter)?;
    let requester = next_account_info(account_iter)?;
    let execution_account = next_account_info(account_iter)?;
    let system_program = next_account_info(account_iter)?;
    let bonsol_program = next_account_info(account_iter)?;
    let image_id_account = next_account_info(account_iter)?;
    let program_id_account = next_account_info(account_iter)?;

    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Verify escrow account
    let (expected_escrow_pda, _) = Pubkey::find_program_address(&[seed], program_id);
    if escrow_account.key != &expected_escrow_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    // Check if escrow is already claimed
    let escrow_data = escrow_account.try_borrow_data()?;
    let escrow = EscrowAccount::unpack(&escrow_data)?;
    drop(escrow_data);

    if escrow.is_claimed {
        return Err(ProgramError::Custom(1)); // Already claimed error
    }

    // Verify requester PDA
    let (expected_requester, bump2) =
        Pubkey::find_program_address(&[execution_id.as_bytes()], program_id);
    if requester.key != &expected_requester {
        return Err(ProgramError::InvalidSeeds);
    }

    // Create requester account if it doesn't exist
    if requester.lamports() == 0 {
        let space = ExecutionTracker::SIZE + 100;
        let rent = Rent::get()?;
        let lamports = rent.minimum_balance(space);

        let create_account_ix = system_instruction::create_account(
            payer.key,
            requester.key,
            lamports,
            space as u64,
            program_id,
        );

        invoke_signed(
            &create_account_ix,
            &[payer.clone(), requester.clone(), system_program.clone()],
            &[&[execution_id.as_bytes(), &[bump2]]],
        )?;
    }

    let clock = Clock::get()?;
    let expiration = clock.slot.saturating_add(expiry_offset);

    msg!("execution_id: {}, tip: {}, expiration: {}, preimage: {:?}", execution_id, tip, expiration, preimage);

    // Prepare Bonsol execution
    let bonsol_ix = execute_v1(
        payer.key,
        payer.key,
        SHA256_IMAGE_ID,
        execution_id,
        vec![
            InputRef::url(preimage), // The preimage to hash
            InputRef::private(PRIVATE_DATA_URL),
        ],
        tip,
        expiration,
        ExecutionConfig {
            verify_input_hash: false,
            input_hash: None,
            forward_output: true,
        },
        Some(CallbackConfig {
            program_id: *program_id,
            instruction_prefix: vec![2], // handle_claim_callback instruction
            extra_accounts: vec![
                AccountMeta::new(*requester.key, false),      // requester
                AccountMeta::new(*escrow_account.key, false), // escrow_account (writable)
                AccountMeta::new(*receiver.key, false),       // receiver (writable)
            ],
        }),
        None,
    )
    .map_err(|_| ProgramError::InvalidInstructionData)?;

    msg!("requester: {:?}, balance: {:?}", requester.key, requester.lamports());
    msg!("payer: {:?}, balance: {:?}", payer.key, payer.lamports());
    msg!("system_program: {:?}, balance: {:?}", system_program.key, system_program.lamports());
    msg!("execution_account: {:?}, balance: {:?}", execution_account.key, execution_account.lamports());
    msg!("bonsol_program: {:?}, balance: {:?}", bonsol_program.key, bonsol_program.lamports());
    msg!("image_id_account: {:?}, balance: {:?}", image_id_account.key, image_id_account.lamports());
    msg!("escrow_account: {:?}, balance: {:?}", escrow_account.key, escrow_account.lamports());
    msg!("receiver: {:?}, balance: {:?}", receiver.key, receiver.lamports());
    msg!("program_id_account: {:?}, balance: {:?}", program_id_account.key, program_id_account.lamports());

    msg!("bump: {}, bump2: {}", bump, bump2);

    invoke_signed(
        &bonsol_ix,
        &[
            requester.clone(),          // requester
            payer.clone(),              // payer
            system_program.clone(),     // system_program
            execution_account.clone(),  // execution_account
            bonsol_program.clone(),     // bonsol_program
            image_id_account.clone(),   // image_id
            escrow_account.clone(),     // escrow_account (for callback)
            receiver.clone(),           // receiver (for callback)
            program_id_account.clone(), // program_id (our program)
        ],
        &[&[execution_id.as_bytes(), &[bump2]]],
    )?;

    // Store execution account reference
    let mut requester_data = requester.try_borrow_mut_data()?;
    let tracker = ExecutionTracker {
        execution_account: *execution_account.key,
    };
    tracker.pack(&mut requester_data)?;

    msg!("Claim request submitted with execution ID: {}", execution_id);
    Ok(())
}

// Instruction 2: Handle callback from Bonsol
pub fn handle_claim_callback(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    msg!("Handling claim callback...");

    if accounts.len() < 4 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let requester = &accounts[1];
    let escrow_account = &accounts[2];
    let receiver = &accounts[3];

    if !escrow_account.is_writable || !receiver.is_writable {
        return Err(ProgramError::InvalidInstructionData);
    }

    let requester_data = requester.try_borrow_data()?;
    let execution_account = Pubkey::try_from(&requester_data[0..32])
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    let callback_output: BonsolCallback = handle_callback(
        SHA256_IMAGE_ID,
        &execution_account,
        accounts,
        data,
    )
    .map_err(|_| ProgramError::InvalidInstructionData)?;

    if callback_output.committed_outputs.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    msg!("Callback committed outputs length: {:?}", callback_output.committed_outputs.len());
    msg!("Callback committed outputs (bytes): {:?}", callback_output.committed_outputs);

    // Parse the hash result from Bonsol
    if let Ok(computed_hash_str) = std::str::from_utf8(&callback_output.committed_outputs) {
        msg!("Computed hash from Bonsol: {}", computed_hash_str);

        // Load and verify escrow
        let mut escrow_data = escrow_account.try_borrow_mut_data()?;
        let mut escrow = EscrowAccount::unpack(&escrow_data)?;

        if escrow.is_claimed {
            return Err(ProgramError::Custom(1)); // Already claimed
        }

        // Convert stored hash bytes to string for comparison
        let stored_hash_str = std::str::from_utf8(&escrow.hash)
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        msg!("Stored hash in escrow: {}", stored_hash_str);

        // Compare hashes
        if computed_hash_str.trim() == stored_hash_str.trim() {
            msg!("Hash verification successful! Releasing escrow...");

            // Transfer lamports from escrow to receiver
            let transfer_lamports = escrow.amount_lamports;
            
            **escrow_account.try_borrow_mut_lamports()? -= transfer_lamports;
            **receiver.try_borrow_mut_lamports()? += transfer_lamports;

            // Update escrow state
            escrow.is_claimed = true;
            escrow.receiver = Some(*receiver.key);
            escrow.pack(&mut escrow_data)?;

            msg!(
                "Escrow claimed successfully! Transferred {} lamports to {}",
                transfer_lamports,
                receiver.key
            );
        } else {
            msg!("Hash verification failed! Expected: {}, Got: {}", stored_hash_str, computed_hash_str);
            return Err(ProgramError::Custom(2)); // Hash mismatch error
        }
    } else {
        msg!("Could not parse hash from callback output");
        return Err(ProgramError::InvalidInstructionData);
    }

    Ok(())
}