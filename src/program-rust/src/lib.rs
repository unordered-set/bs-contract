use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::{Pubkey, PUBKEY_BYTES},
    clock::{UnixTimestamp, Clock},
    program_pack::{IsInitialized, Pack, Sealed},
    program_memory::{sol_memcmp, sol_memset},
    sysvar::{rent::Rent, Sysvar},
};
use borsh::{BorshDeserialize, BorshSerialize};
use arrayref::{array_mut_ref, array_ref, array_refs, mut_array_refs};


use borsh::maybestd::{
    io::{Error, ErrorKind, Result as BorshResult, Write},
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatchOutcome {
    Unknown,
    TeamA,
    TeamB,
    Draw
}

impl Default for MatchOutcome {
    fn default() -> Self {
        MatchOutcome::Unknown
    }
}

impl BorshSerialize for MatchOutcome {
    fn serialize<W: Write>(&self, writer: &mut W) -> BorshResult<()> {
        match self {
            MatchOutcome::Unknown => 0u8.serialize(writer),
            MatchOutcome::TeamA => 1u8.serialize(writer),
            MatchOutcome::TeamB => 2u8.serialize(writer),
            MatchOutcome::Draw => 3u8.serialize(writer),
        }
    }
}

impl BorshDeserialize for MatchOutcome {
    fn deserialize(buf: &mut &[u8]) -> BorshResult<Self> {
        if buf.len() != 1 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Unexpected length of input",
            ));
        }
        match buf[0] {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::TeamA),
            2 => Ok(Self::TeamB),
            3 => Ok(Self::Draw),
            _ => Err(Error::new(ErrorKind::InvalidInput, "MatchOutcome_bad_input"))
        }
    }
}

// #[repr(C)]
// #[derive(Clone, Copy, Debug, Default, PartialEq)]
// pub struct Bet {
//     betor: Pubkey,
//     lamports: u64,
//     expected_outcome: MatchOutcome,
// }

// #[repr(C)]
// #[derive(Clone, Copy, Debug, Default, PartialEq)]
// pub struct EventBets {
//     is_initialized: bool,                  // 1
//     arbiter: Pubkey,                       // 32
//     bets_allowed_until_ts: UnixTimestamp,  // 8
//     outcome: MatchOutcome,                   // 8
//     total_bets: usize,                     // 8
//     bets_raw: [u8; 48000], // Lazy unpack of [Bet,1000];  // 1000 * (32 + 8 + 8)
// }

// impl Default for EventBets {
//     fn default() -> Self {
//         is_initialized: bool(),
//         arbiter: Pubkey(),
//         bets_allowed_until_ts: UnixTimestamp(),
//         total_bets: 0,
//         bets_raw: [0; 48000],
//     }
// }

#[derive(BorshSerialize, BorshDeserialize)]
pub struct EventBets {
    pub is_initialized: bool,
    pub arbiter: Pubkey,
    pub bets_allowed_until_ts: UnixTimestamp,
    pub outcome: u8,
    pub bets_betors: Vec<Pubkey>,
    pub bets_lamports: Vec<u64>,
    pub bets_outcomes: Vec<u8>,
}

fn pack_match_outcome(value: MatchOutcome) -> u8{
    match value {
        MatchOutcome::Unknown => 0,
        MatchOutcome::TeamA => 1,
        MatchOutcome::TeamB => 2,
        MatchOutcome::Draw => 3,
    }
}
fn unpack_match_outcome(src: u8) -> Result<MatchOutcome, ProgramError> {
    match src {
        0 => Ok(MatchOutcome::Unknown),
        1 => Ok(MatchOutcome::TeamA),
        2 => Ok(MatchOutcome::TeamB),
        3 => Ok(MatchOutcome::Draw),
        _ => Err(ProgramError::InvalidAccountData),
    }
}

/*
impl Sealed for EventBets {}

fn pack_public_key(key: &Pubkey, dst: &mut [u8; 32]) {
    dst.copy_from_slice(key.as_ref());
}
fn unpack_public_key(src: &[u8; 32]) -> Pubkey {
    Pubkey::new_from_array(*src)
}




impl Pack for EventBets {
    const LEN: usize = 48057;
    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let src = array_ref![src, 0, 48057];
        let (is_initialized, arbiter, bets_allowed_until_ts, outcome, total_bets, bets_raw) =
            array_refs![src, 1, 32, 8, 8, 8, 48000];
        let is_initialized = match is_initialized {
            [0] => false,
            [1] => true,
            _ => return Err(ProgramError::InvalidAccountData),
        };
        let outcome = match outcome[0] {
            0 => MatchOutcome::Unknown,
            1 => MatchOutcome::TeamA,
            2 => MatchOutcome::TeamB,
            3 => MatchOutcome::Draw,
            _ => return Err(ProgramError::InvalidAccountData),
        };
        Ok(EventBets{
            is_initialized,
            arbiter: unpack_public_key(arbiter),
            bets_allowed_until_ts: i64::from_le_bytes(*bets_allowed_until_ts),
            outcome,
            total_bets: usize::from_le_bytes(*total_bets),
            bets_raw: *bets_raw,
        })
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, 48056];
        let (
            //is_initialized_dst,
            arbiter_dst,
            bets_allowed_until_ts_dst,
            result_dst,
            total_bets_dst,
            bets_raw_dst
        ) = mut_array_refs![dst, 32, 8, 8, 8, 48000];
        *is_initialized_dst = [self.is_initialized as u8];
        pack_public_key(&self.arbiter, arbiter_dst);
        *bets_allowed_until_ts_dst = self.bets_allowed_until_ts.to_le_bytes();
        result_dst[0] = pack_match_outcome(self.outcome);
        *total_bets_dst = self.total_bets.to_le_bytes();
        *bets_raw_dst = self.bets_raw;
    }
}
*/


#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    // Checks and initializes an empty account.
    // Accepted accounts:
    //    [readable, signed] - owner account, signed, mostly to avoid fat finger errors.
    //    [writable] - bets account
    Initialize,

    // Adds a bet
    // Accepted accounts:
    //    [writable] - betor
    //    [writable] - bets account
    AddBet{
        choice: MatchOutcome,
    },

    // Sets a winner
    // Accepted accounts
    //    [readable, signer] - owner account
    //    [writable] - bets account
    SetWinner{
        result: MatchOutcome,
    },

    // Withdraw your win
    //    [readable] - betor (no need to be signed, bc. it's ok if someone else decides to withdraw for you)
    //    [writable] - bets account
    Withdraw,
}

impl Instruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        use std::convert::TryInto;
        use ProgramError::InvalidInstructionData;
        let (&tag, rest) = input.split_first().ok_or(InvalidInstructionData)?;
        Ok(match tag {
            0 => Self::Initialize,
            1 => {
                let (&choice, rest) = rest.split_first().ok_or(InvalidInstructionData)?;
                Self::AddBet { choice: unpack_match_outcome(choice)? }
            },
            2 => {
                let (&result, rest) = rest.split_first().ok_or(InvalidInstructionData)?;
                Self::SetWinner { result: unpack_match_outcome(result)? }
            },
            3 => Self::Withdraw,
            _ => unreachable!()
        })
    }
}

pub fn cmp_pubkeys(a: &Pubkey, b: &Pubkey) -> bool {
    sol_memcmp(a.as_ref(), b.as_ref(), PUBKEY_BYTES) == 0
}

fn _process_initialize(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    msg!("Instruction: _process_initialize");
    let account_info_iter = &mut accounts.iter();
    let owner = next_account_info(account_info_iter)?;
    if (!owner.is_signer) {
        msg!("Instruction: _process_initialize: wrong signer");
        return Err(ProgramError::MissingRequiredSignature)
    }
    
    let bets_info = next_account_info(account_info_iter)?;
    let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
    if !rent.is_exempt(bets_info.lamports(), bets_info.data_len()) {
        msg!("Instruction: _process_initialize: no exempt, size={}", bets_info.data_len());
        return Err(ProgramError::InvalidAccountData)
    }

    if !cmp_pubkeys(program_id, bets_info.owner) {
        msg!("Instruction: _process_initialize: wrong owner");
        return Err(ProgramError::InvalidAccountData)
    }

    if bets_info.data_len() < 48057 {
        msg!("Instruction: _process_initialize: buffer is too small");
        return Err(ProgramError::InvalidAccountData)
    }
    
    //let mut bets = EventBets::unpack_unchecked(&bets_info.data.borrow())?;
    let mut bets = EventBets::deserialize(&mut &bets_info.data.borrow()[..])?;
    msg!("Instruction: _process_initialize: Unpacked");
    if bets.is_initialized {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    msg!("Instruction: _process_initialize: Initializing...");

    bets.is_initialized = true;
    bets.arbiter = *owner.key;
    bets.outcome = 0u8;
    let timeout: i64 = 5 * 60;
    bets.bets_allowed_until_ts = Clock::get()?.unix_timestamp + timeout;

    // EventBets::pack(bets, &mut bets_info.data.borrow_mut())?;
    bets.serialize(&mut &mut bets_info.data.borrow_mut()[..])?;

    Ok(())
}

fn _process_add_bet(program_id: &Pubkey, accounts: &[AccountInfo], choice: MatchOutcome, lamports: u64) -> ProgramResult {
    msg!("Adding {} for resolution {}", lamports, pack_match_outcome(choice));

    let account_info_iter = &mut accounts.iter();
    // let _ = next_account_info(account_info_iter)?;
    let betor = next_account_info(account_info_iter)?; 
    msg!("Betor = {}", betor.key);
    let bets_info = next_account_info(account_info_iter)?;
    msg!("bets_info = {}", bets_info.key);
    let tmp_storage_key = next_account_info(account_info_iter)?;
    msg!("tmp_storage_key = {}", tmp_storage_key.key);

    if !cmp_pubkeys(program_id, bets_info.owner) {
        msg!("Instruction: _process_add_bet: wrong owner for event {}", bets_info.owner);
        return Err(ProgramError::InvalidAccountData)
    }
    if !cmp_pubkeys(program_id, tmp_storage_key.owner) {
        msg!("Instruction: _process_add_bet: wrong owner for tmp storage");
        return Err(ProgramError::InvalidAccountData)
    }
    let mut bets = EventBets::deserialize(&mut &bets_info.data.borrow()[..])?;
    if !bets.is_initialized {
        msg!("Instruction: _process_add_bet: not Initialized...");
        return Err(ProgramError::InvalidAccountData);
    }
    if Clock::get()?.unix_timestamp > bets.bets_allowed_until_ts {
        msg!("Instruction: _process_add_bet: too late, bets are no longer accepted");
        return Err(ProgramError::InvalidAccountData);
    }

    bets.bets_outcomes.push(pack_match_outcome(choice));
    bets.bets_betors.push(*betor.key);
    bets.bets_lamports.push(tmp_storage_key.lamports());
    msg!("Sending funds from {} to {}", tmp_storage_key.key, bets_info.key);
    **bets_info.try_borrow_mut_lamports()? += tmp_storage_key.lamports();
    **tmp_storage_key.try_borrow_mut_lamports()? = 0;

    bets.serialize(&mut &mut bets_info.data.borrow_mut()[..])?;
    Ok(())
}

fn _process_set_winner(program_id: &Pubkey, accounts: &[AccountInfo], result: MatchOutcome) -> ProgramResult {
    Ok(())
}

fn _process_withdraw(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    Ok(())
}


// Declare and export the program's entrypoint
entrypoint!(process_instruction);

// Program entrypoint's implementation
pub fn process_instruction(
    program_id: &Pubkey, // Public key of the account the hello world program was loaded into
    accounts: &[AccountInfo], // The account to say hello to
    _instruction_data: &[u8], // Ignored, all helloworld instructions are hellos
) -> ProgramResult {
    let instruction = Instruction::unpack(_instruction_data)?;

    match instruction {
        Instruction::Initialize => _process_initialize(program_id, accounts),
        Instruction::AddBet{choice} => _process_add_bet(program_id, accounts, choice),
        Instruction::SetWinner{result} => _process_set_winner(program_id, accounts, result),
        Instruction::Withdraw => _process_withdraw(program_id, accounts),
    }
}

// Sanity tests
#[cfg(test)]
mod test {
    use super::*;
    use solana_program::clock::Epoch;
    use std::mem;

    #[test]
    fn test_sanity() {
        let program_id = Pubkey::default();
        let key = Pubkey::default();
        let mut lamports = 0;
        let mut data = vec![0; mem::size_of::<u32>()];
        let owner = Pubkey::default();
        let account = AccountInfo::new(
            &key,
            false,
            true,
            &mut lamports,
            &mut data,
            &owner,
            false,
            Epoch::default(),
        );
        let instruction_data: Vec<u8> = Vec::new();

        let accounts = vec![account];

        assert_eq!(
            GreetingAccount::try_from_slice(&accounts[0].data.borrow())
                .unwrap()
                .counter,
            0
        );
        process_instruction(&program_id, &accounts, &instruction_data).unwrap();
        assert_eq!(
            GreetingAccount::try_from_slice(&accounts[0].data.borrow())
                .unwrap()
                .counter,
            1
        );
        process_instruction(&program_id, &accounts, &instruction_data).unwrap();
        assert_eq!(
            GreetingAccount::try_from_slice(&accounts[0].data.borrow())
                .unwrap()
                .counter,
            2
        );
    }
}
