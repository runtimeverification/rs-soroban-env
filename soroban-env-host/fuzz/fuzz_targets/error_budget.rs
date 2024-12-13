use std::collections::BTreeMap;

use arbitrary::Arbitrary;
use honggfuzz::fuzz;
use soroban_env_host::{
    xdr::{
        HostFunction, InvokeContractArgs, ScErrorCode,
        ScErrorType, ScSymbol, ScVal
    },
    Host, StorageType,
};
use soroban_test_wasms::ERROR_BUDGET_TEST;
use soroban_test_wasms::HOSTILE;

// We augment the `Expr` we generate with other parameters we'd like the fuzzer to explore.
#[derive(Arbitrary, Debug)]
struct TestCase {
    cpu_budget: u32,
    mem_budget: u32,
    data_keys: BTreeMap<u8, (StorageType, bool)>,
}

impl TestCase {
    fn install_budget(&self, host: &Host) {
        host.with_budget(|budget| {
            // Mask the budget down to 268m instructions / 256MiB memory so we don't
            // literally run out of time or memory on the machine we're fuzzing on;
            // but also mask it _up_ to at least 1m instructions / 1MiB memory so
            // we don't just immediately fail instantiating the VM.
            budget.reset_limits(
                self.cpu_budget as u64 & 0x0fff_ffff | 0x000f_ffff,
                self.mem_budget as u64 & 0x0ff_ffff | 0x000f_ffff,
            )
        })
        .unwrap();
    }
}

fn main() {
    loop {
        fuzz!(|test: TestCase| {
            let data_keys = test
                .data_keys
                .iter()
                .map(|(k, v)| (ScVal::U32(*k as u32), v.clone()))
                .take(1)
                .collect::<BTreeMap<_, _>>();

            let (host, contracts, signers) = Host::new_recording_fuzz_host(
                &[ERROR_BUDGET_TEST, HOSTILE],
                &data_keys,
                1,
            );

            let contract_address_a = host.scaddress_from_address(contracts[0]).unwrap();
            let contract_address_b = host.scaddress_from_address(contracts[1]).unwrap();

            let args_a: Vec<ScVal> = vec![ScVal::Address(contract_address_b,)];

            let hf = HostFunction::InvokeContract(InvokeContractArgs {
                contract_address: contract_address_a,
                function_name: ScSymbol("run".try_into().unwrap()),
                args: args_a.try_into().unwrap(),
            });

            // First pass: recording.
            test.install_budget(&host);
            let _ = host.invoke_function(hf.clone());

            // Second pass: enforcing (with synthesized content as needed).
            host.switch_fuzz_host_to_enforcing(&data_keys, &signers);
            test.install_budget(&host);
            let res = host.invoke_function(hf);

            // Non-internal error-code returns are ok, we are interested in _panics_ and
            // internal errors.
            if let Err(hosterror) = res {
                if hosterror.error.is_code(ScErrorCode::InternalError)
                    && !hosterror.error.is_type(ScErrorType::Contract)
                {
                    panic!("got internal error: {:?}", hosterror)
                }
            }
        });
    }
}
