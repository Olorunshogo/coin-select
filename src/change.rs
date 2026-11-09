#[allow(unused)] // some bug in <= 1.48.0 sees this as unused when it isn't
use crate::float::FloatExt;
use crate::{varint_size, FeeRate, TR_KEYSPEND_TXIN_WEIGHT, TR_SPK_WEIGHT, TXOUT_BASE_WEIGHT};

/// Represents the weight costs of a change output.
///
/// May also represent multiple outputs.
#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ChangeWeights {
    /// The weight of including this change output.
    ///
    /// This must not take into account any weight change from varint output count.
    pub output_weight: u64,
    /// The weight of spending this change output (in the future).
    pub spend_weight: u64,
    /// The total number of outputs that the change will use
    pub n_outputs: usize,
}

impl ChangeWeights {
    /// `ChangeWeights` for an output that will be spent with a taproot keyspend
    pub const TR_KEYSPEND: Self = Self {
        output_weight: TXOUT_BASE_WEIGHT + TR_SPK_WEIGHT,
        spend_weight: TR_KEYSPEND_TXIN_WEIGHT,
        n_outputs: 1,
    };

    /// `ChangeWeights` for no change at all
    pub const NONE: Self = Self {
        output_weight: 0,
        spend_weight: 0,
        n_outputs: 0,
    };

    /// The waste of adding this change to a transaction according to the [waste metric].
    ///
    /// To get the precise answer you need to pass in the number of non-change outputs (`n_target_outputs`) that you're
    /// adding to the transaction so we can include the cost of increasing the varint size of the output length.
    ///
    /// [waste metric]: https://bitcoin.stackexchange.com/questions/113622/what-does-waste-metric-mean-in-the-context-of-coin-selection
    pub fn waste(
        &self,
        feerate: FeeRate,
        long_term_feerate: FeeRate,
        n_target_outputs: usize,
    ) -> f64 {
        let extra_varint_weight =
            (varint_size(n_target_outputs + self.n_outputs) - varint_size(n_target_outputs)) * 4;
        let extra_output_weight = self.output_weight + extra_varint_weight;
        extra_output_weight as f64 * feerate.spwu()
            + self.spend_weight as f64 * long_term_feerate.spwu()
    }

    /// The fee you will pay to spend these change output(s) in the future.
    pub fn spend_fee(&self, long_term_feerate: FeeRate) -> u64 {
        (self.spend_weight as f64 * long_term_feerate.spwu()).ceil() as u64
    }

    /// The minimum value a change output with these weights must have to not be considered dust
    /// according to `dust_relay_feerate`.
    ///
    /// A change output is dust when the fee to relay it plus the fee to later spend it exceeds its
    /// value, so the threshold is `dust_relay_feerate` applied to the output weight plus the spend
    /// weight.
    pub fn dust_threshold(&self, dust_relay_feerate: FeeRate) -> u64 {
        dust_relay_feerate.implied_fee_wu(self.output_weight + self.spend_weight)
    }
}

/// A change output.
/// Technically it could represent multiple outputs.
///
/// This is returned from [`CoinSelector::change`]. Note if `change` returns a change where `is_none()`
/// returns true then **no change should be added** to the transaction.
///
/// [`CoinSelector::change`]: crate::CoinSelector::change
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Change {
    /// Weight of adding change output and spending the change output.
    pub weights: ChangeWeights,
    /// The value that should be assigned to the change.
    pub value: u64,
}

impl Change {
    /// The change which represents no change at all. We could but don't use `Option` because this
    /// causes friction internally, instead we just use a `Change` with all 0 values.
    pub const NONE: Self = Change {
        weights: ChangeWeights::NONE,
        value: 0,
    };

    /// is the "none" change
    pub fn is_none(&self) -> bool {
        self == &Change::NONE
    }

    /// Is not the "none" change
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
}

/// Describes when a change output (although it could represent several) should be added that drains
/// the excess in the coin selection. It includes the `change_weights` to account for the cost of
/// adding this outupt(s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChangePolicy {
    /// The minimum amount of excess there needs to be add a change output.
    pub min_value: u64,
    /// The weights of the change that would be added according to the policy.
    pub change_weights: ChangeWeights,
}

impl ChangePolicy {
    /// Construct a change policy that creates change when the change value is greater than
    /// `min_value`.
    pub fn min_value(change_weights: ChangeWeights, min_value: u64) -> Self {
        Self {
            change_weights,
            min_value,
        }
    }

    /// Construct a change policy that creates change when it would reduce the transaction waste
    /// given that `min_value` is respected.
    pub fn min_value_and_waste(
        change_weights: ChangeWeights,
        min_value: u64,
        target_feerate: FeeRate,
        long_term_feerate: FeeRate,
    ) -> Self {
        // The output waste of a changeless solution is the excess.
        let waste_with_change = change_weights
            .waste(
                target_feerate,
                long_term_feerate,
                0, /* ignore varint cost for now */
            )
            .ceil() as u64;

        Self {
            change_weights,
            min_value: waste_with_change.max(min_value),
        }
    }
}
