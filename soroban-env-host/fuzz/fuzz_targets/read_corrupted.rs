//! Linear memory calls with fake `Caller`/`Vm` environment
//! 
//! This test: 
//! - write an arbitrary Map to linear memory
//! - corrupt some random bytes within the written bytes
//! - try to read back the Map
//! - expect: graceful failure, no InternalError, no panic.

use honggfuzz::fuzz;
use honggfuzz::arbitrary::Arbitrary;

use std::cmp::Ordering;

use soroban_env_host::*;
use soroban_env_host::budget::AsBudget;
use soroban_env_host::xdr::{Hash, ScContractInstance, ScErrorCode, ContractExecutable};
use soroban_env_host::{Vm, Frame, MeteredOrdMap, valid_scval::ValidScVal};

use soroban_synth_wasm::ModEmitter;

fn main() {
    let Ok(()) = mocked_linear_memory() else { panic!("Program errored")};
}

// No `InternalError`s are allowed to happen, the host must reject invalid data.
fn panic_if_internal_error(e: HostError) -> HostError {
    if e.error.is_code(ScErrorCode::InternalError) {
        panic!("Operation caused an internal error, aborting.")
    }
    e
}

macro_rules! ensure_no_internal_error {
    ($result:expr) => {
        $result.map_err(panic_if_internal_error)?
    }
}

#[derive(Arbitrary)]
struct InputData {
    // data to write
    values: Vec<ValidScVal>,
    // LM address to write keys to
    keys_address: u16, // may overflow
    // location and data to modify linear memory contents after writing
    scramble_offset: u8,
    scramble_bytes: Vec<u8>,
    // budget parameters
    budget_cpu: u16, // to scale with 2^10, 100M considered "sane", see soroban-env-host::budget::limits
    budget_mem: u16, // to scale with 2^10, 40M considered "sane" (for an entire transaction)
    // selecting a potential crash scenario
    crash_selector: u8, // which potentially-crashing test to run
}

fn mocked_linear_memory() -> Result<(), HostError> {

    // make a wasm module that exports "memory"
    let fake_wasm = &ModEmitter::default_with_test_protocol().finish()[..];
    let fake_hash: Hash = Hash([0;32]);

    loop {
        // we need to use a fresh host for every iteration, a static host would
        // retain all created objects and grow a huge memory footprint over time
        let host = &Host::test_host();
        let vm = Vm::new(host, fake_hash.clone(), fake_wasm)?;

        let frame =
            Frame::ContractVM{
                vm: vm.clone(),
                fn_name: Symbol::try_from_small_str("symbol")?,
                args: vec![],
                instance: ScContractInstance{executable: ContractExecutable::Wasm(fake_hash.clone()), storage: None},
                relative_objects: vec![]
            };

        fuzz!( |input: InputData| {

            let _ = host.with_frame(frame.clone(), || {

                // set budget to random values
                fn scale(n:u16) -> u64 { (n as u64) << 10 }
                host.as_budget().reset_limits(scale(input.budget_cpu), scale(input.budget_mem))?;

                // produce a simple map from the given vector of valid values
                let mut keys_vec: Vec<Val> = vec![];

                // cannot sort `Val` easily so we sort the input
                let length = input.values.len();
                let mut sc_vals = input.values;
                sc_vals.sort_by(|a, b| a.0.cmp(&b.0));

                for e in sc_vals.into_iter() {
                    let v: Val = ensure_no_internal_error!(host.map_err(<Val>::try_from_val(host, &e.0)));
                    keys_vec.push(v)
                }
                // keys are directly used, values are the keys rotated by one position (if any)
                let mut values_vec = keys_vec.clone();
                if values_vec.len() > 0 {
                    values_vec[..].rotate_left(1);
                }

                // make a new map of k1 -> k2, k2 -> k3, ..., k<n> -> k1
                // let theMap = ensure_no_internal_error!(host.map_new_from_slices(keys_vec[..], values_vec));
                // unfortunately this only works with symbols as keys.
                let map_vec: Vec<(Val, Val)> = keys_vec.into_iter().zip(values_vec.iter().copied()).collect();
                let map_obj = host.add_host_object(MeteredOrdMap::from_map(map_vec, host)?)?;

                vm.with_vmcaller(|vmcaller| {

                    // write map keys and values to vectors, then call map_unpack_to_l.m. with keys vector.
                    // Values array should be identical.
                    let keys_obj = ensure_no_internal_error!(VmCallerEnv::map_keys(
                        host,
                        vmcaller,
                        map_obj
                    ));
                    let _ = ensure_no_internal_error!(VmCallerEnv::vec_unpack_to_linear_memory(
                        host,
                        vmcaller,
                        keys_obj,
                        (input.keys_address as u32).into(),
                        (length as u32).into(), // why does the API require length? 
                    ));
                    let values_obj = ensure_no_internal_error!(VmCallerEnv::map_values(
                        host,
                        vmcaller,
                        map_obj
                    ));
                    let values_address = (input.keys_address as usize + length) as u32;
                    let _ = ensure_no_internal_error!(VmCallerEnv::map_unpack_to_linear_memory(
                        host,
                        vmcaller,
                        map_obj,
                        (input.keys_address as u32).into(),
                        values_address.into(),
                        (length as u32).into(),
                    ));
                    let values_read = ensure_no_internal_error!(VmCallerEnv::vec_new_from_linear_memory(
                        host,
                        vmcaller,
                        values_address.into(),
                        (length as u32).into(), 
                    ));
                    // compare value arrays
                    let should_equal = ensure_no_internal_error!(host.compare(&values_obj, &values_read));
                    assert_eq!(Ordering::Equal, should_equal);

                    // produce a new map from keys and values

                     // overwrite keys array with extracted values
                    let _ = ensure_no_internal_error!(VmCallerEnv::map_unpack_to_linear_memory(
                        host,
                        vmcaller,
                        map_obj,
                        (input.keys_address as u32).into(),
                        (input.keys_address as u32).into(), // perfect overlap since values are just the keys
                        (length as u32).into(),
                    ));
                    // use the written values again as keys
                    let _ = ensure_no_internal_error!(VmCallerEnv::map_unpack_to_linear_memory(
                        host,
                        vmcaller,
                        map_obj,
                        (input.keys_address as u32).into(),
                        (input.keys_address as u32).into(), // overwrite keys again
                        (length as u32).into(),
                    ));



                    // crashes. choose one
                    let bytes_obj = ensure_no_internal_error!(
                        host.bytes_new_from_slice(&input.scramble_bytes[..])
                    );
                    match input.crash_selector % 4 {
                        0 => Ok(map_obj.into()), // no error, no rollback

                        // try to use corrupted keys
                        1 => {
                            // corrupt the keys vector (bytes_copy_to_l.m.)
                            let _ = ensure_no_internal_error!(VmCallerEnv::bytes_copy_to_linear_memory(
                                host,
                                vmcaller,
                                bytes_obj,
                                0.into(),
                                (input.keys_address as u32 + input.scramble_offset as u32).into(),
                                (input.scramble_bytes.len() as u32).into()
                            ));
                            // try map_unpack_to_l.m. with corrupted keys. Could crash
                            let _ = ensure_no_internal_error!(VmCallerEnv::map_unpack_to_linear_memory(
                                host,
                                vmcaller,
                                map_obj,
                                (input.keys_address as u32).into(),
                                values_address.into(),
                                (length as u32).into(),
                            ));
                                    
                            Ok(map_obj.into()) // probably not reached
                        },

                        // corrupt the keys vector (bytes_copy_to_l.m.) then try map_new_from_l.m. Could crash
                        2 => { 
                            // corrupt the keys vector (bytes_copy_to_l.m.)
                            let _ = ensure_no_internal_error!(VmCallerEnv::bytes_copy_to_linear_memory(
                                host,
                                vmcaller,
                                bytes_obj,
                                0.into(),
                                (input.keys_address as u32 + input.scramble_offset as u32).into(),
                                (input.scramble_bytes.len() as u32).into()
                            ));
                            // try map_new_from_l.m. Could crash
                            let new_map = ensure_no_internal_error!(VmCallerEnv::map_new_from_linear_memory(
                                host,
                                vmcaller,
                                (input.keys_address as u32).into(),
                                values_address.into(),
                                (length as u32).into(),
                            ));

                            Ok(new_map.into()) // probably not reached
                        },

                        // corrupt the values vector, then try map_new_from_l.m. Could crash
                        3 => { 
                            // corrupt the keys vector (bytes_copy_to_l.m.)
                            let _ = ensure_no_internal_error!(VmCallerEnv::bytes_copy_to_linear_memory(
                                host,
                                vmcaller,
                                bytes_obj,
                                0.into(),
                                (values_address + input.scramble_offset as u32).into(),
                                (input.scramble_bytes.len() as u32).into()
                            ));
                            // try map_new_from_l.m. Could crash
                            let new_map = ensure_no_internal_error!(VmCallerEnv::map_new_from_linear_memory(
                                host,
                                vmcaller,
                                (input.keys_address as u32).into(),
                                values_address.into(),
                                (length as u32).into(),
                            ));

                            Ok(new_map.into())
                        },
                        n => panic!("unexpected value {n} (modulo computation is broken)")
                    }

                // return an error at the end so the frame will be rolled back (no persisting host objects)
                // Err(HostError::from(Error::from_type_and_code(ScErrorType::Contract, ScErrorCode::InternalError)))
              })
            });
        });
    }
    // never returns unless in error
}
