//! One step of learning, in tensors.
//!
//! Two losses added together. The policy is pushed towards the deed it took in
//! proportion to how much better what followed was than the value head
//! expected; the value head is pushed towards what actually followed. The
//! first is what learns to play and the second is what makes the first mean
//! anything, because "better than expected" needs somebody to be expecting.
//!
//! A third term keeps the policy from making its mind up too early. Without
//! it, a policy that has found one deed that pays will sharpen onto it until
//! it can no longer try anything else, and it stops learning while its loss
//! goes on falling — which looks exactly like progress.

use candle_core::{DType, Device, Result as Chance, Tensor};

use crate::{DEEDS, INPUT, Model};

/// How much the value head's error counts against the policy's.
const VALUE_SHARE: f64 = 0.5;
/// How much keeping an open mind counts.
const OPEN_MIND: f64 = 0.01;
/// What a masked deed's number is dragged down to.
///
/// Not the whole way to nothing: an exponent of minus infinity would put a
/// nought where a gradient has to be worked out through, and the arithmetic
/// stops being arithmetic.
const SHUT_OUT: f64 = -1e9;

/// The loss of one batch of decisions.
///
/// `rows` is the numbers of every frame laid end to end, `masks` is a one for
/// every deed that was **not** allowed, and `worths` is what followed each
/// decision.
pub fn a_step(
    model: &Model,
    rows: &[f32],
    chosen: &[u32],
    worths: &[f32],
    masks: &[f32],
    count: usize,
) -> Chance<Tensor> {
    let device = Device::Cpu;
    let rows = Tensor::from_vec(rows.to_vec(), (count, INPUT), &device)?;
    let (liking, worth) = model.run(&rows)?;

    // What could not be done is put out of reach before anything is compared,
    // so it can never be chosen and never carries a gradient.
    let shut = Tensor::from_vec(masks.to_vec(), (count, DEEDS), &device)?;
    let liking = (liking + (shut * SHUT_OUT)?)?;

    // The log of what the policy thinks of each deed.
    let highest = liking.max_keepdim(1)?;
    let steady = liking.broadcast_sub(&highest)?;
    let total = steady.exp()?.sum_keepdim(1)?.log()?;
    let logs = steady.broadcast_sub(&total)?;

    let taken = Tensor::from_vec(chosen.to_vec(), count, &device)?;
    let mine = logs.gather(&taken.reshape((count, 1))?, 1)?;

    let followed = Tensor::from_vec(worths.to_vec(), (count, 1), &device)?;
    // How much better than expected. The value head is not moved by this half,
    // or it would learn to expect little and call everything a surprise.
    let surprise = (&followed - &worth.detach())?;

    let told_so = (mine * &surprise)?.mean_all()?.neg()?;
    let expecting = (worth - &followed)?.sqr()?.mean_all()?;
    // What an open mind is worth: the spread of the policy over what it may
    // do, which is highest when it has not made its mind up.
    let openness = (logs.exp()? * &logs)?.sum(1)?.mean_all()?;

    let loss = (told_so + (expecting * VALUE_SHARE)?)?;
    (loss + (openness * OPEN_MIND)?)?.to_dtype(DType::F32)
}
