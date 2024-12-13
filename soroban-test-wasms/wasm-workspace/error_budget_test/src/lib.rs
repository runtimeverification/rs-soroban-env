#![no_std]
use soroban_sdk::{contract, contractimpl, vec, Address, Bytes, Env, Val, Vec, symbol_short, Error};

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    pub fn run(env: Env, hostile_contract: Address) {
        let _ = env.try_invoke_contract::<u32, Error>(
            &hostile_contract,
            &symbol_short!("oob1"),
            Vec::new(&env),
        );

        let _ = env.try_invoke_contract::<u32, Error>(
            &hostile_contract,
            &symbol_short!("oob2"),
            Vec::new(&env),
        );

        let forgeref1 = 0x12345678 as u32;
        let forgeref2 = 0x87654321 as u32;
        let _ = env.try_invoke_contract::<u32, Error>(
            &hostile_contract,
            &symbol_short!("forgeref"),
            vec![&env, forgeref1.into(), forgeref2.into()],
        );

        let val_vec: Val = vec![&env, 123_u32, 456_u32, 789_u32].into();

        let _ = env.try_invoke_contract::<u32, Error>(
            &hostile_contract,
            &symbol_short!("forgety1"),
            vec![&env, val_vec],
        );

        let b = Bytes::from_slice(&env, &[1,2,3,4,5]);
        let _ = env.try_invoke_contract::<u32, Error>(
            &hostile_contract,
            &symbol_short!("forgety2"),
            vec![&env, vec![&env, b].into()],
        );

        let _ = env.try_invoke_contract::<u32, Error>(
            &hostile_contract,
            &symbol_short!("badtag"),
            vec![&env, val_vec],
        );

        let _ = env.try_invoke_contract::<u32, Error>(
            &hostile_contract,
            &symbol_short!("pushbad"),
            vec![&env, val_vec, 0xaaaaaaaa_u32.into(), 0xbbbbbbbb_u32.into()],
        );

        let _ = env.try_invoke_contract::<u32, Error>(
            &hostile_contract,
            &symbol_short!("idxbad"),
            vec![&env, val_vec, 0xaaaaaaaa_u32.into(), 0xbbbbbbbb_u32.into()],
        );
    }
}
