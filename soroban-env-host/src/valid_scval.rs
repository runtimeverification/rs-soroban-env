// soroban-env-host/src/valid_scval.rs

use arbitrary::*;
use arbitrary::Error::NotEnoughData;
use unstructured::ArbitraryIter;
use crate::xdr::*;

#[derive(Debug)]
pub struct ValidScSymbol(pub ScSymbol);

impl From<ValidScSymbol> for ScSymbol {
    fn from(valid: ValidScSymbol) -> Self { valid.0 }
}

impl Arbitrary<'_> for ValidScSymbol {
    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        size_hint::and(
            usize::size_hint(depth),
            (0, Some(SCSYMBOL_LIMIT as usize))
        )
    }

    // generate a symbol containing only valid characters and respecting the symbol maximum length
    fn arbitrary(u: &mut Unstructured) -> Result<ValidScSymbol> {

        const SYMBOL_CHARS: &[u8] =
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
            .as_bytes();

        let mut len: usize = Arbitrary::arbitrary(u)?;
        len = len % (SCSYMBOL_LIMIT as usize);
        // FIXME is zero length allowed?

        let mut result: Vec<u8> = Vec::with_capacity(len);
        for _ in 0..len {
            let c: &u8 = u.choose(SYMBOL_CHARS)?;
            result.push(c.clone());
        }
        let sym_str = StringM::try_from(result).unwrap();
        Ok(ValidScSymbol(ScSymbol(sym_str)))
    }
}

fn arbitrary_vec(u: &mut Unstructured) -> Result<Vec<ScVal>> {
    let result: ArbitraryIter<'_, '_, ValidScVal> = u.arbitrary_iter()?;

    result.map(|res| res.map(|e| ScVal::from(e))).collect()
}

fn choice(n:u32, u: &mut Unstructured) -> Result<u64> {
    // this is how generated Arbitrary instances choose
    Ok((u64::from(<u32 as Arbitrary>::arbitrary(u)?) * u64::from(n)) >> 32)
}
// macro for selecting from alternatives with this?

#[derive(Debug)]
pub struct ValidScVal(pub ScVal);

impl From<ValidScVal> for ScVal {
    fn from(valid: ValidScVal) -> Self { valid.0 }
}

impl Arbitrary<'_> for ValidScVal {

        fn size_hint(depth: usize) -> (usize, Option<usize>) {
            size_hint::recursion_guard(depth, |depth| {
                size_hint::or_all(
                    &[ bool::size_hint(depth)
                    , (0, Some(0))
                    , (ScError::size_hint(depth))
                    , u32::size_hint(depth)
                    , i32::size_hint(depth)
                    , u64::size_hint(depth)
                    , i64::size_hint(depth)
                    // also covers Timepoint and Duration
                    , UInt128Parts::size_hint(depth)
                    , Int128Parts::size_hint(depth)
                    , UInt256Parts::size_hint(depth)
                    , Int256Parts::size_hint(depth)
                    , ScBytes::size_hint(depth) // recursive
                    , ScString::size_hint(depth) // recursive
                    , ValidScSymbol::size_hint(depth)
                    , (4, None) // recursive but consuming at least some bytes for length
                    , (4, None) // ditto
                    , ScAddress::size_hint(depth)
                    ])
            })
        }

        fn arbitrary<'a>(u: &mut Unstructured<'a>) -> Result<Self> {

            // check that we aren't running empty
            if u.is_empty() {
                return Err(NotEnoughData);
            }

            let chosen = choice(19, u)?;
            let val = match chosen {
                0 => ScVal::Bool(Arbitrary::arbitrary(u)?),
                1 => ScVal::Void,
                2 => ScVal::Error(Arbitrary::arbitrary(u)?),
                3 => ScVal::U32(Arbitrary::arbitrary(u)?),
                4 => ScVal::I32(Arbitrary::arbitrary(u)?),
                5 => ScVal::U64(Arbitrary::arbitrary(u)?),
                6 => ScVal::I64(Arbitrary::arbitrary(u)?),
                7 => ScVal::Timepoint(Arbitrary::arbitrary(u)?),
                8 => ScVal::Duration(Arbitrary::arbitrary(u)?),
                9 => ScVal::U128(Arbitrary::arbitrary(u)?),
                10 => ScVal::I128(Arbitrary::arbitrary(u)?),
                11 => ScVal::U256(Arbitrary::arbitrary(u)?),
                12 => ScVal::I256(Arbitrary::arbitrary(u)?),
                13 => ScVal::Bytes(Arbitrary::arbitrary(u)?),
                14 => ScVal::String(Arbitrary::arbitrary(u)?),
                // three cases with custom invariants:
                // symbol with limited character set
                15 => ScVal::Symbol(ValidScSymbol::arbitrary(u)?.0),
                // recursion (needs guard) and bogus Option
                16 => {
                        let elems = arbitrary_vec(u)?;
                        ScVal::Vec(Some(ScVec(VecM::try_from(elems).unwrap())))
                },
                // recursive (needs guard), bogus option, invariant: sorted by keys, no duplicates
                17 => {
                    let keys = arbitrary_vec(u)?;

                    let mut elems: Vec<ScMapEntry> = 
                        keys.into_iter().map(|k| {
                            let v: ValidScVal = 
                                Arbitrary::arbitrary(u)
                                    // use key as value if running out of randomness here
                                    .unwrap_or_else(|_| ValidScVal(k.clone()));
                            ScMapEntry{ key: k, val: v.0 }
                        }).collect();

                    elems.sort_by(|a, b| a.key.cmp(&b.key));
                    elems.dedup_by_key(|e| e.key.clone());
                    let vec = VecM::try_from(elems).unwrap();
                    ScVal::Map(Some(ScMap(vec)))
                },
                // last harmless case
                18 => ScVal::Address(Arbitrary::arbitrary(u)?),
                // invalid cases, not selected because n = 19 above
                // 19 => ScVal::LedgerKeyContractInstance,
                // 20 => ScVal::LedgerKeyNonce(Arbitrary::arbitrary(u)?),
                // 21 => ScVal::ContractInstance(Arbitrary::arbitrary(u)?),
                _ => panic!("internal error: entered unreachable code")
            };
            Ok(ValidScVal(val))
        }
}

#[cfg(test)]
use crate::{Host, HostError, Env, DEFAULT_XDR_RW_LIMITS};

#[test]
pub 
fn test_all_valid() -> Result<(), HostError> {
    // see crate::test::bytes::arbitrary_xdr_roundtrips, but insists on valid ScVals
    use rand::RngCore;
    use soroban_env_common::Compare;
    const ITERATIONS: u32 = 50_000;
    let host = Host::test_host_with_prng();
    host.budget_ref().reset_unlimited().unwrap();

    let mut successes = 0;
    let mut failures = 0;
    let mut roundtrip_test = |v: ScVal| -> Result<(), HostError> {
        let bytes: Vec<u8> = v.to_xdr(DEFAULT_XDR_RW_LIMITS)?;
        let scval_bytes_obj = host
            .add_host_object(ScBytes(bytes.try_into().unwrap()))
            .unwrap();
        // We expect the input to be a valid ScVal because we generate it using a custom generator
        let deserialize_res = host.deserialize_from_bytes(scval_bytes_obj);
        match deserialize_res {
            Ok(val) => {
                let serialized_bytes = host.serialize_to_bytes(val)?;
                assert_eq!(
                    host.compare(&scval_bytes_obj, &serialized_bytes)?,
                    core::cmp::Ordering::Equal
                );
                successes += 1;
            }
            Err(err) => {
                assert!(
                    err.error.is_code(ScErrorCode::UnexpectedType)
                        || err.error.is_code(ScErrorCode::InvalidInput)
                );
                eprintln!("Found invalid value: {v:?}");
                failures += 1;
            }
        }
        Ok(())
    };
    for _ in 0..ITERATIONS {
        let mut data = vec![0u8; 5000];
        host.with_test_prng(|rng| {
            rng.fill_bytes(data.as_mut_slice());
            Ok(())
        })?;

        let ValidScVal(sc_val) = ValidScVal::arbitrary(&mut Unstructured::new(data.as_slice())).unwrap();
        roundtrip_test(sc_val)?;
    }

    assert_eq!(failures, 0);
    Ok(())
}
