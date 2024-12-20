//! Linear memory calls with fake `Caller`/`Vm` environment

use honggfuzz::fuzz;

use soroban_env_host::*;
use soroban_env_host::budget::AsBudget;

use soroban_env_host::xdr::{Hash, ScContractInstance, ScErrorType, ScErrorCode, ContractExecutable};

use soroban_env_host::{Vm, Error, Frame};

use soroban_synth_wasm::ModEmitter;

fn main() {
    let Ok(()) = mocked_linear_memory() else { panic!("Program errored")};
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

        fuzz!( |input: &[u8]| {

            let _ = host.with_frame(frame.clone(), || {

                if input.len() > 2 {
                    // Experiment: set CPU and memory limits before linear memory operations
                    // Expect: gracefully failing or succeeding, no crash.
                    let limit = as_u16(input[0], input[1]) as u64;
                    host.as_budget().reset_limits(limit, limit)?;
                } else {
                    host.as_budget().reset_unlimited()?;
                }
    
                vm.with_vmcaller(|vmcaller| {

                let data =
                    if input.len() > 65535 {
                        &input[..65535] // restrict data length to one memory page
                    } else {
                        input
                    };
                let test_len = data.len() as u32;

                // write and read as string
                let test_str: StringObject =
                    host.string_new_from_slice(data)?;

                let _ = VmCallerEnv::string_copy_to_linear_memory(
                    host,
                    vmcaller,
                    test_str,
                    0.into(),
                    0.into(),
                    test_len.into()
                )?;

                let new_str = VmCallerEnv::string_new_from_linear_memory(
                    host,
                    vmcaller,
                    0.into(),
                    test_len.into()
                )?;

                let cmp = VmCallerEnv::obj_cmp(host, vmcaller, test_str.into(), new_str.into())?;
                assert_eq!(cmp, 0);

                // write partially as bytes and read back
                // needs 4 bytes of random data to work with
                if test_len > 4 {
                    let test_bytes: BytesObject =
                    host.bytes_new_from_slice(data)?;

                    // write a random amount (but not too much) starting at a random
                    // position within test data, to the _end_ of the memory page we have available.

                    let write_start = as_u16(data[0], data[1]) % test_len;
                    let write_len = as_u16(data[2], data[3]) % (test_len - write_start);
                    let target_addr = 65535 - write_len;

                    let _ = VmCallerEnv::bytes_copy_to_linear_memory(
                        host,
                        vmcaller,
                        test_bytes,
                        write_start.into(),
                        target_addr.into(),
                        write_len.into()
                    )?;

                    // read back what was written
                    let bytes_read = VmCallerEnv::bytes_new_from_linear_memory(
                        host,
                        vmcaller,
                        target_addr.into(),
                        write_len.into()
                    )?;
                    let byte_piece: BytesObject =
                        host.bytes_new_from_slice(&data[(write_start as usize)..((write_start + write_len) as usize)])?;
                    assert_eq!(0, VmCallerEnv::obj_cmp(host, vmcaller, bytes_read.into(), byte_piece.into())?);


                    // write the same things back from memory into the original
                    let same_bytes = VmCallerEnv::bytes_copy_from_linear_memory(
                        host,
                        vmcaller,
                        test_bytes,
                        write_start.into(),
                        target_addr.into(),
                        write_len.into()
                    )?;
                    let cmp = VmCallerEnv::obj_cmp(host, vmcaller, test_bytes.into(), same_bytes.into())?;
                    assert_eq!(cmp, 0);
                }

                // return an error at the end so the frame will be rolled back (no persisting host objects)
                Err(HostError::from(Error::from_type_and_code(ScErrorType::Contract, ScErrorCode::InternalError)))
              })
            });
        });
    }
    // never returns unless in error
}

fn as_u16(data1: u8, data2: u8) -> u32 {
    ((data1 as u32) << 8) + (data2 as u32)
}
