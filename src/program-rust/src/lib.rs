use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::{Pubkey, PUBKEY_BYTES},
    clock::{UnixTimestamp, Clock},
    program_memory::{sol_memcmp},
    sysvar::{rent::Rent, Sysvar},
};
use borsh::{BorshDeserialize, BorshSerialize};

const COMISSION: u8 = 3;


use borsh::maybestd::{
    io::{Error, ErrorKind, Result as BorshResult, Write},
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatchOutcome {
    Unknown,
    TeamA,
    TeamB,
    Draw,
    Withdrawn,
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
            MatchOutcome::Withdrawn => 255u8.serialize(writer),
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
            255 => Ok(Self::Withdrawn),
            _ => Err(Error::new(ErrorKind::InvalidInput, "MatchOutcome_bad_input"))
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct EventBets {
    pub is_initialized: bool,
    pub arbiter: Pubkey,
    pub bets_allowed_until_ts: UnixTimestamp,
    pub outcome: u8,
    pub balance_a: u64,
    pub balance_b: u64,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Bet {
    pub is_initialized: bool,
    pub betor: Pubkey,
    pub event: Pubkey,
    pub amount: u64,
    pub outcome: u8,
}

const BETS_RENT_EXCEMPTION: u64 = 1405920;

fn pack_match_outcome(value: MatchOutcome) -> u8{
    match value {
        MatchOutcome::Unknown => 0,
        MatchOutcome::TeamA => 1,
        MatchOutcome::TeamB => 2,
        MatchOutcome::Draw => 3,
        MatchOutcome::Withdrawn => 255,
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

#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    // Checks and initializes an empty account.
    // Accepted accounts:
    //    [readable, signed] - owner account, signed, mostly to avoid fat finger errors.
    //    [writable] - bets account
    Initialize{
        bets_accepted_until: UnixTimestamp,
    },

    // Adds a bet
    // Accepted accounts:
    //    [writable] - betor
    //    [writable] - bets account
    //    [writable] - tmp account with SOLs to deposit
    //    [writable] - bet info
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
    //    [writable] - bet info
    Withdraw,
}

impl Instruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        use std::convert::TryInto;
        use ProgramError::InvalidInstructionData;
        let (&tag, rest) = input.split_first().ok_or(InvalidInstructionData)?;
        Ok(match tag {
            0 => {
                let bets_accepted_until = rest
                    .get(..8)
                    .and_then(|slice| slice.try_into().ok())
                    .map(UnixTimestamp::from_le_bytes)
                    .ok_or(InvalidInstructionData)?;
                Self::Initialize { bets_accepted_until }
            },
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

fn _process_initialize(program_id: &Pubkey, bets_accepted_until: UnixTimestamp, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let owner = next_account_info(account_info_iter)?;
    if !owner.is_signer {
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
    
    let mut bets = EventBets::deserialize(&mut &bets_info.data.borrow()[..])?;
    if bets.is_initialized {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    if bets_accepted_until < Clock::get()?.unix_timestamp {
        msg!("Bets accepted until {} but now it is {}", bets_accepted_until, Clock::get()?.unix_timestamp);
        return Err(ProgramError::InvalidInstructionData);
    }

    bets.is_initialized = true;
    bets.arbiter = *owner.key;
    bets.outcome = 0u8;
    bets.bets_allowed_until_ts = bets_accepted_until;
    bets.balance_a = 0;
    bets.balance_b = 0;

    bets.serialize(&mut &mut bets_info.data.borrow_mut()[..])?;
    Ok(())
}

fn _process_add_bet(program_id: &Pubkey, accounts: &[AccountInfo], choice: MatchOutcome) -> ProgramResult {
    // What can go wrong?
    // `bets_info_acc` does not belong to our program, and someone scams our users.
    // `this_bet_acc` does not belong to our program, again possible scam, but actually don't think it is achievable.
    // `this_bet_acc` does not have enough funds to be rent excepmpted. Pretty bad, users may be disappointed.
    // `bets_info` is wrong, uninitnalized - users can be scammed by betting to something else.
    // `bets_info.bets_allowed_until_ts` is in the past.
    // `bets_info.outcome` is not yet set (it should not, but just in case)...

    let account_info_iter = &mut accounts.iter();
    let betor = next_account_info(account_info_iter)?; 
    let bets_info_acc = next_account_info(account_info_iter)?;
    let this_bet_acc = next_account_info(account_info_iter)?;

    msg!("betor = {}, bets_info = {}, this_bet_acc = {}", betor.key, bets_info_acc.key, this_bet_acc.key);
    if !cmp_pubkeys(program_id, bets_info_acc.owner) {
        msg!("Instruction: _process_add_bet: wrong owner for event {}", bets_info_acc.owner);
        return Err(ProgramError::InvalidAccountData)
    }
    if !cmp_pubkeys(program_id, this_bet_acc.owner) {
        msg!("Instruction: _process_add_bet: wrong owner for event {}", this_bet_acc.owner);
        return Err(ProgramError::InvalidAccountData)
    }
    
    let mut bets = EventBets::deserialize(&mut &bets_info_acc.data.borrow()[..])?;
    let mut this_bet = Bet::deserialize(&mut &this_bet_acc.data.borrow()[..])?;
    if !bets.is_initialized {
        msg!("Instruction: _process_add_bet: BetInfo should be Initialized...");
        return Err(ProgramError::InvalidAccountData);
    }
    if this_bet.is_initialized {
        msg!("Instruction: _process_add_bet: Bet is already Initialized...");
        return Err(ProgramError::InvalidAccountData);
    }
    if Clock::get()?.unix_timestamp > bets.bets_allowed_until_ts {
        msg!("Instruction: _process_add_bet: too late, bets are no longer accepted");
        return Err(ProgramError::InvalidAccountData);
    }
    if unpack_match_outcome(bets.outcome)? != MatchOutcome::Unknown {
        msg!("Betting on completed match");
        return Err(ProgramError::InvalidAccountData);
    }

    msg!("Adding {} for resolution {}", this_bet_acc.lamports(), pack_match_outcome(choice));
    this_bet.is_initialized = true;
    this_bet.outcome = pack_match_outcome(choice);
    this_bet.betor = *betor.key;
    this_bet.amount = this_bet_acc.lamports() - BETS_RENT_EXCEMPTION;
    this_bet.event = *bets_info_acc.key;

    match choice {
        MatchOutcome::TeamA => { bets.balance_a += this_bet.amount; },
        MatchOutcome::TeamB => { bets.balance_b += this_bet.amount; },
        _ => { return Err(ProgramError::InvalidAccountData); },
    };

    msg!("Sending funds from {} to {}", this_bet_acc.key, bets_info_acc.key);
    **bets_info_acc.try_borrow_mut_lamports()? += this_bet.amount;
    **this_bet_acc.try_borrow_mut_lamports()? = BETS_RENT_EXCEMPTION;

    bets.serialize(&mut &mut bets_info_acc.data.borrow_mut()[..])?;
    this_bet.serialize(&mut &mut this_bet_acc.data.borrow_mut()[..])?;
    Ok(())
}

fn _process_set_winner(program_id: &Pubkey, accounts: &[AccountInfo], result: MatchOutcome) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let owner = next_account_info(account_info_iter)?; 
    let bets_info = next_account_info(account_info_iter)?;
    if !owner.is_signer {
        msg!("Instruction: _process_set_winner: wrong signer");
        return Err(ProgramError::MissingRequiredSignature)
    }
    let mut bets = EventBets::deserialize(&mut &bets_info.data.borrow()[..])?;
    if !bets.is_initialized {
        msg!("Instruction: _process_set_winner: not Initialized...");
        return Err(ProgramError::InvalidAccountData);
    }
    if Clock::get()?.unix_timestamp < bets.bets_allowed_until_ts {
        msg!("Instruction: _process_set_winner: too early");
        return Err(ProgramError::InvalidAccountData);
    }
    if !cmp_pubkeys(&bets.arbiter, owner.key) {
        msg!("Instruction: _process_set_winner: you are not an arbiter");
        return Err(ProgramError::InvalidAccountData);
    }
    if result == MatchOutcome::Unknown {
        msg!("Can not set result back to Unknown");
        return Err(ProgramError::InvalidAccountData);
    }
    
    if unpack_match_outcome(bets.outcome)? == MatchOutcome::Unknown {
        msg!("Sending funds from {} to {}", bets_info.key, owner.key);
        let comission: u64 = bets_info.lamports() * (COMISSION as u64) / 100u64;
        **bets_info.try_borrow_mut_lamports()? -= comission;
        **owner.try_borrow_mut_lamports()? += comission;
    }
    bets.outcome = pack_match_outcome(result);
    bets.serialize(&mut &mut bets_info.data.borrow_mut()[..])?;

    Ok(())
}

fn _process_withdraw(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let betor = next_account_info(account_info_iter)?; 
    let bets_info = next_account_info(account_info_iter)?;
    let this_bet_acc = next_account_info(account_info_iter)?;

    msg!("betor = {}, bets_info = {}, this_bet_acc = {}", betor.key, bets_info.key, this_bet_acc.key);
    
    if !cmp_pubkeys(program_id, bets_info.owner) {
        msg!("Instruction: _process_add_bet: wrong owner for event {}", bets_info.owner);
        return Err(ProgramError::InvalidAccountData)
    }
    
    let bets = EventBets::deserialize(&mut &bets_info.data.borrow()[..])?;
    let mut this_bet = Bet::deserialize(&mut &this_bet_acc.data.borrow()[..])?;

    if !cmp_pubkeys(bets_info.key, &this_bet.event) {
        msg!("Bet does not match event");
        return Err(ProgramError::InvalidAccountData)
    }
    if this_bet.betor != *betor.key {
        msg!("Withdrawing to foreigner account");
        return Err(ProgramError::InvalidAccountData);
    }
    if unpack_match_outcome(bets.outcome)? != MatchOutcome::Unknown {
        msg!("Betting on completed match");
        return Err(ProgramError::InvalidAccountData);
    }

    let withdraw_balance = match (unpack_match_outcome(bets.outcome)?, unpack_match_outcome(this_bet.outcome)?) {
        (MatchOutcome::TeamA, MatchOutcome::TeamA) => {
            let mut result = 1u128;
            result *= bets.balance_b as u128;
            result *= this_bet.amount as u128;
            result /= bets.balance_a as u128;
            result += this_bet.amount as u128;
            result *= (100-COMISSION) as u128;
            result /= 100u128;
            result
        },
        (MatchOutcome::TeamB, MatchOutcome::TeamB) => {
            let mut result = 1u128;
            result *= bets.balance_a as u128;
            result *= this_bet.amount as u128;
            result /= bets.balance_b as u128;
            result += this_bet.amount as u128;
            result *= (100-COMISSION) as u128;
            result /= 100u128;
            result
        },
        (MatchOutcome::Draw, MatchOutcome::TeamA) | (MatchOutcome::Draw, MatchOutcome::TeamB)=> {
            let mut result = 0u128;
            result += this_bet.amount as u128;
            result *= (100-COMISSION) as u128;
            result /= 100u128;
            result
        },
        _ => 0
    };

    if withdraw_balance > bets_info.lamports().into() {
        msg!("Withdrawing too much: {}", withdraw_balance);
        return Err(ProgramError::InvalidAccountData);
    }

    this_bet.outcome = pack_match_outcome(MatchOutcome::Withdrawn);
    msg!("Sending {} lamports from {} to {}", withdraw_balance, bets_info.key, betor.key);
    **bets_info.try_borrow_mut_lamports()? -= withdraw_balance as u64;
    **betor.try_borrow_mut_lamports()? += withdraw_balance as u64;

    bets.serialize(&mut &mut bets_info.data.borrow_mut()[..])?;
    this_bet.serialize(&mut &mut this_bet_acc.data.borrow_mut()[..])?;

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
    msg!("UNpacked");

    match instruction {
        Instruction::Initialize{bets_accepted_until} => _process_initialize(program_id, bets_accepted_until, accounts),
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
