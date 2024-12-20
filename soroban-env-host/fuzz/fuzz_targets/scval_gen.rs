//! Find out how many invalid ScVal we are generating


use honggfuzz::fuzz;

use soroban_env_host::*;
use soroban_env_host::budget::AsBudget;
use soroban_env_host::valid_scval::ValidScVal;
use soroban_env_host::xdr::ScErrorCode;

fn main() {
    loop {
        fuzz!(|input:ValidScVal| {
            let scval = input.0;

            let env = &Host::test_host();
            env.as_budget().reset_unlimited().unwrap();

            match Val::try_from_val(env, &scval) {
                Ok(_rawval_1) => return (),
                Err(e) if e.is_code(ScErrorCode::ExceededLimit) => 
                    return (),
                Err(e) =>
                    // We should only generate valid ScVals:
                    panic!("Invalid ScVal generated: {scval:?}, {e:?}")
                }
            })
    }
}
