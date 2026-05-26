//! Proof lemmas about `2^exp` (`pow2_nat`) and the executable
//! `pow2_u64_exec` / `coin_value_pow2_exec` helpers.
//!
//! The lemmas establish monotonicity (`e1 <= e2 ==> pow2_nat(e1) <=
//! pow2_nat(e2)`) and the saturating bound (`pow2_nat(30) == 2^30 ==
//! 1073741824`). The exec helpers compute `2^exp` under the
//! `exp <= MAX_EXPONENT` precondition, with `res <= 1073741824u64`
//! as a postcondition for downstream overflow reasoning.

use vstd::prelude::*;

use crate::*;

verus! {

/// Spec-only lemma: `pow2_nat` is monotone (non-decreasing). Proved by
/// straightforward induction on the exponent.
pub proof fn lemma_pow2_monotone(e1: nat, e2: nat)
    requires
        e1 <= e2,
    ensures
        pow2_nat(e1) <= pow2_nat(e2),
    decreases e2,
{
    if e2 == 0 {
        // e1 == 0 too; trivially equal.
    } else if e1 == e2 {
        // trivial
    } else {
        lemma_pow2_monotone(e1, (e2 - 1) as nat);
    }
}

/// Spec-only lemma: `pow2_nat(30) == 2^30 = 1073741824`. Unrolled
/// once-per-step (Verus's default fuel is 1, so a single recursive
/// step). Used to derive the u64-overflow-safety bound for
/// `pow2_u64_exec`.
pub proof fn lemma_pow2_at_30()
    ensures
        pow2_nat(30) == 1073741824nat,
{
    reveal_with_fuel(pow2_nat, 31);
}

/// Executable real coin value (Quint `coinValue`): `2^exp` for
/// `exp <= MAX_EXPONENT`. Thin convenience wrapper over
/// [`pow2_u64_exec`] that matches the `coin_value_pow2` spec fn.
pub fn coin_value_pow2_exec(exp: u8) -> (res: u64)
    requires
        exp <= MAX_EXPONENT,
    ensures
        res as nat == coin_value_pow2(exp),
{
    pow2_u64_exec(exp)
}

/// Executable `2^exp` for `exp <= MAX_EXPONENT` (= 30). Returns the
/// real Quint `coinValue` for that exponent. Verus-verified
/// overflow-safe: `MAX_EXPONENT = 30 ⇒ 2^30 < u64::MAX`.
///
/// This is the foundational primitive for switching the pilot's
/// `coin_value(exp) = exp + 1` scheme over to real `2^exp` arithmetic
/// (task #84). Existing aggregations still use the pilot scheme — this
/// just gives callers (and a future rewrite) the safe building block.
pub fn pow2_u64_exec(exp: u8) -> (res: u64)
    requires
        exp <= MAX_EXPONENT,
    ensures
        res as nat == pow2_nat(exp as nat),
        res <= 1073741824u64,
{
    let mut result: u64 = 1;
    let mut k: u8 = 0;
    while k < exp
        invariant
            k <= exp,
            exp <= MAX_EXPONENT,
            result as nat == pow2_nat(k as nat),
            result <= 1073741824u64,
        decreases exp - k,
    {
        proof {
            // Bound `result * 2` by 2^30 = 1073741824 to keep within u64.
            // After this iteration, k+1 <= exp <= 30, so
            // pow2(k+1) <= pow2(30) = 2^30.
            lemma_pow2_at_30();
            lemma_pow2_monotone((k as nat) + 1, MAX_EXPONENT as nat);
        }
        result = result * 2;
        k = k + 1;
    }
    result
}

} // verus!
