//! Find out how many invalid ScVal we are generating


use honggfuzz::fuzz;

use soroban_env_host::*;
use soroban_env_host::budget::AsBudget;
use soroban_env_host::valid_scval::ValidScVal;
use soroban_env_host::xdr::{ScBytes, ScErrorCode, WriteXdr};

fn main() {

    let host = &Host::test_host();

    loop {
        fuzz!(|input:ValidScVal| {
            let scval = input.0;

            host.as_budget().reset_unlimited().unwrap();

            swallow_exceeded_limit_errors( || {

                let bytes: Vec<u8> = scval.to_xdr(DEFAULT_XDR_RW_LIMITS)?;

                let scval_bytes_obj = host
                    .add_host_object(ScBytes(bytes.try_into().unwrap()))
                    .unwrap();

                // We expect the input to be a valid ScVal because we generate it using a custom generator
                let deserialize_res = soroban_env_host::Env::deserialize_from_bytes(host, scval_bytes_obj)?;
                                   // host.deserialize_from_bytes(scval_bytes_obj);

                let serialized_bytes = soroban_env_host::Env::serialize_to_bytes(host, deserialize_res)?;
                              // host.serialize_to_bytes(val)?;

                let result = host.compare(&scval_bytes_obj, &serialized_bytes)?;

                assert_eq!(result, core::cmp::Ordering::Equal);
        
                Ok(())
            });
        })
    }
}

fn swallow_exceeded_limit_errors<F>( f: F) 
where
    F: FnOnce () -> Result<(), HostError>, 
{
    match f() {
        Ok(a) => a,
        Err(e) if e.error.is_code(ScErrorCode::ExceededLimit) => return (),
        Err(e) => panic!("Unexpected error {e:?}")
    }
}
