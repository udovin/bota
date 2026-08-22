//! Moving the weights.
//!
//! Adam, written out rather than taken from a library: it is fifteen lines and
//! it keeps everything the tensor crate touches inside two files.

use candle_core::{Result as Chance, Tensor};

use crate::Model;

/// How much of the last average of the gradient is kept.
const KEEP_FIRST: f64 = 0.9;
/// How much of the last average of its square is kept.
const KEEP_SECOND: f64 = 0.999;
/// What keeps a step from dividing by nothing.
const STEADY: f64 = 1e-8;

/// What an optimiser remembers between steps.
pub struct Adam {
    /// How far a step goes.
    pub rate: f32,
    /// The running average of the gradient, per weight.
    first: Vec<Tensor>,
    /// The running average of its square.
    second: Vec<Tensor>,
    /// Steps taken, for the warm-up correction.
    steps: f32,
}

impl Adam {
    /// An optimiser for one model.
    pub fn new(model: &Model, rate: f32) -> Chance<Adam> {
        let mut first = Vec::new();
        let mut second = Vec::new();
        for weight in model.weights() {
            first.push(weight.as_tensor().zeros_like()?);
            second.push(weight.as_tensor().zeros_like()?);
        }
        Ok(Adam {
            rate,
            first,
            second,
            steps: 0.0,
        })
    }

    /// Moves every weight one step against the gradient of a loss.
    pub fn step(&mut self, model: &Model, loss: &Tensor) -> Chance<()> {
        let grads = loss.backward()?;
        self.steps += 1.0;
        let warm_first = 1.0 - KEEP_FIRST.powf(f64::from(self.steps));
        let warm_second = 1.0 - KEEP_SECOND.powf(f64::from(self.steps));
        for (at, weight) in model.weights().into_iter().enumerate() {
            let Some(grad) = grads.get(weight) else {
                continue;
            };
            let first = ((&self.first[at] * KEEP_FIRST)? + (grad * (1.0 - KEEP_FIRST))?)?;
            let second =
                ((&self.second[at] * KEEP_SECOND)? + (grad.sqr()? * (1.0 - KEEP_SECOND))?)?;
            let step = ((&first / warm_first)? / ((&second / warm_second)?.sqrt()? + STEADY)?)?;
            weight.set(&(weight.as_tensor() - (step * f64::from(self.rate))?)?)?;
            // Cut loose from how they were worked out. Kept attached, each
            // step's averages point at the last step's, and letting go of them
            // after a few thousand steps walks a chain that deep and takes the
            // stack with it.
            self.first[at] = first.detach();
            self.second[at] = second.detach();
        }
        Ok(())
    }
}
