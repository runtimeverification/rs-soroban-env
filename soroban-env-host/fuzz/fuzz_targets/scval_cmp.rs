//! Check that pairs of SCVal compare similar to converting them to Val 
//! and comparing the Val. Uses TryFromVal trait.
//! 


use honggfuzz::fuzz;
use arbitrary::Arbitrary;

use soroban_env_host::*;
use soroban_env_host::budget::AsBudget;
use soroban_env_host::valid_scval::ValidScVal;
use soroban_env_host::xdr::ScVal;
use std::cmp::Ordering;

#[derive(Arbitrary)]
struct ScPair {
    a: ValidScVal,
    b: ValidScVal
}

macro_rules! assert_eq {
    ($left:expr, $right:expr) => {
        if $left != $right {
            panic!("Expected to be equal but is not"); // FIXME output the arguments
        }
    };
}

fn main() {
    loop {
        fuzz!(|input:ScPair| {
            let scval_1 = input.a.0;
            let scval_2 = input.b.0;

            let env = &Host::test_host();
            env.as_budget().reset_unlimited().unwrap();

            // Compare Ord & PartialOrd
            let scval_cmp = Ord::cmp(&scval_1, &scval_2);
            let scval_cmp_partial = PartialOrd::partial_cmp(&scval_1, &scval_2);
    
            assert_eq!(Some(scval_cmp), scval_cmp_partial);
    
            let rawval_1 = Val::try_from_val(env, &scval_1);
            let rawval_1 = match rawval_1 {
                Ok(rawval_1) => rawval_1,
                Err(_) => {
                    // Many ScVal's are invalid:
                    //
                    // - LedgerKeyNonce
                    // - Vec(None), Map(None)
                    // - Symbol with invalid chars
                    // - Map with duplicate keys
                    // - Containers with the above
                    return ();
                }
            };
    
            let rawval_2 = Val::try_from_val(env, &scval_2);
            let rawval_2 = match rawval_2 {
                Ok(rawval_2) => rawval_2,
                Err(_) => {
                    return ();
                }
            };
    
            let rawval_cmp = env.compare(&rawval_1, &rawval_2).expect("cmp");
    
            if scval_cmp != rawval_cmp {
                panic!(
                    "scval and rawval don't compare the same:\n\
                     {scval_1:#?}\n\
                     {scval_2:#?}\n\
                     {scval_cmp:#?}\n\
                     {rawval_1:#?}\n\
                     {rawval_2:#?}\n\
                     {rawval_cmp:#?}"
                );
            }
    
            // Compare Eq
            let scval_partial_eq = PartialEq::eq(&scval_1, &scval_2);
            let rawval_cmp_is_eq = scval_cmp == Ordering::Equal;
    
            assert_eq!(scval_partial_eq, rawval_cmp_is_eq);
    
            // Compare<ScVal> for Budget
            let budget = env.as_budget();
            let scval_budget_cmp = budget.compare(&scval_1, &scval_2).expect("cmp");
    
            if scval_budget_cmp != scval_cmp {
                panic!(
                    "scval (budget) and scval don't compare the same:\n\
                     {scval_1:#?}\n\
                     {scval_2:#?}\n\
                     {scval_budget_cmp:#?}\n\
                     {scval_cmp:#?}"
                );
            }
    
            // Roundtrip checks
            {
                let scval_after_1 = ScVal::try_from_val(env, &rawval_1);
                let scval_after_1 = match scval_after_1 {
                    Ok(scval_after_1) => scval_after_1,
                    Err(e) => {
                        panic!(
                            "couldn't convert rawval to scval:\n\
                             {rawval_1:?},\n\
                             {scval_1:?},\n\
                             {e:#?}"
                        );
                    }
                };
    
                let scval_cmp_before_after_1 = Ord::cmp(&scval_1, &scval_after_1);
                assert_eq!(scval_cmp_before_after_1, Ordering::Equal);
    
                let scval_after_2 = ScVal::try_from_val(env, &rawval_2);
                let scval_after_2 = match scval_after_2 {
                    Ok(scval_after_2) => scval_after_2,
                    Err(e) => {
                        panic!(
                            "couldn't convert rawval to scval:\n\
                             {rawval_2:?},\n\
                             {scval_2:?},\n\
                             {e:#?}"
                        );
                    }
                };
    
                let scval_cmp_before_after_2 = Ord::cmp(&scval_2, &scval_after_2);
                assert_eq!(scval_cmp_before_after_2, Ordering::Equal);
            }
        })
    }
}