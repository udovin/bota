//! The network, and the only place that knows what it is built with.
//!
//! Everything else hands it rows of numbers and takes back one score a row.
//! Keeping the library behind that seam is deliberate: what the bot decides
//! must not depend on which tensor crate is underneath, and swapping the
//! crate should cost this file rather than the project.
//!
//! The shape is the plainest thing that could work: two hidden layers over a
//! row, and one number out. A row is the tick and one thing the bot could do,
//! so the same weights score every candidate and the count of candidates is
//! free to change from tick to tick.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use candle_core::{DType, Device, Result as Chance, Tensor, Var};

use crate::{Dice, FEATURES};

/// How wide each hidden layer is.
pub const WIDTH: usize = 128;

/// The weights, and the machinery to run and train them.
pub struct Net {
    /// The layers, in order.
    layers: Vec<Layer>,
    /// Where the numbers live.
    device: Device,
}

/// One layer: what it multiplies by and what it adds.
struct Layer {
    /// The weights, `(inputs, outputs)`.
    weight: Var,
    /// The bias, one per output.
    bias: Var,
}

impl Net {
    /// Where trained weights are kept: beside the repository, not inside it.
    ///
    /// Weights are what one machine's teaching arrived at, so they are neither
    /// committed nor carried inside the binary, and a bot asked to play by
    /// them when there are none says so rather than playing something else.
    pub fn path() -> PathBuf {
        crate::beside_the_repository("weights.safetensors")
    }

    /// The network that file holds.
    pub fn learned() -> Chance<Net> {
        Net::from_file(&Net::path(), 1)
    }

    /// A network with weights drawn small and random.
    ///
    /// Drawn from a seed of our own rather than the library's: a training run
    /// that cannot be started again from the same seed is a training run whose
    /// results cannot be checked.
    pub fn fresh(seed: u64) -> Chance<Net> {
        let device = Device::Cpu;
        let mut dice = Dice::from_seed(seed);
        let widths = [FEATURES, WIDTH, WIDTH, 1];
        let mut layers = Vec::new();
        for pair in widths.windows(2) {
            let (into, out) = (pair[0], pair[1]);
            // Spread by the fan-in, so a layer neither shrinks what it is
            // given to nothing nor blows it up.
            let spread = (2.0f32 / into as f32).sqrt();
            let weights: Vec<f32> = (0..into * out).map(|_| dice.spread() * spread).collect();
            layers.push(Layer {
                weight: Var::from_tensor(&Tensor::from_vec(weights, (into, out), &device)?)?,
                bias: Var::from_tensor(&Tensor::zeros(out, DType::F32, &device)?)?,
            });
        }
        Ok(Net { layers, device })
    }

    /// How many numbers the whole of it is.
    pub fn weight_count(&self) -> usize {
        self.layers
            .iter()
            .map(|layer| layer.weight.elem_count() + layer.bias.elem_count())
            .sum()
    }

    /// A score for every row, as a tensor that remembers how it was made.
    ///
    /// The remembering is what training needs; [`Net::scores`] is the same
    /// thing for a bot that only wants the numbers.
    pub fn run(&self, rows: &Tensor) -> Chance<Tensor> {
        let mut out = rows.clone();
        let last = self.layers.len() - 1;
        for (at, layer) in self.layers.iter().enumerate() {
            out = out.matmul(layer.weight.as_tensor())?;
            out = out.broadcast_add(layer.bias.as_tensor())?;
            if at != last {
                out = out.tanh()?;
            }
        }
        // One score a row, rather than a row of one score.
        out.flatten_all()
    }

    /// A score for every candidate, given their rows laid one after another.
    pub fn scores(&self, rows: &[Vec<f32>]) -> Chance<Vec<f32>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let flat: Vec<f32> = rows.iter().flatten().copied().collect();
        let rows = Tensor::from_vec(flat, (rows.len(), FEATURES), &self.device)?;
        self.run(&rows)?.to_vec1()
    }

    /// The rows of one tick as a tensor.
    pub fn tensor_of(&self, rows: &[Vec<f32>]) -> Chance<Tensor> {
        let flat: Vec<f32> = rows.iter().flatten().copied().collect();
        Tensor::from_vec(flat, (rows.len(), FEATURES), &self.device)
    }

    /// Every number the network is made of, for an optimiser to move.
    pub fn weights(&self) -> Vec<&Var> {
        self.layers
            .iter()
            .flat_map(|layer| [&layer.weight, &layer.bias])
            .collect()
    }

    /// Writes the weights out.
    pub fn save(&self, path: &Path) -> Chance<()> {
        let mut named: HashMap<String, Tensor> = HashMap::new();
        for (at, layer) in self.layers.iter().enumerate() {
            named.insert(format!("weight{at}"), layer.weight.as_detached_tensor());
            named.insert(format!("bias{at}"), layer.bias.as_detached_tensor());
        }
        candle_core::safetensors::save(&named, path)
    }

    /// Reads weights back into a network of the same shape.
    pub fn load_from(&mut self, held: &HashMap<String, Tensor>) -> Chance<()> {
        for (at, layer) in self.layers.iter_mut().enumerate() {
            if let Some(weight) = held.get(&format!("weight{at}")) {
                layer.weight.set(weight)?;
            }
            if let Some(bias) = held.get(&format!("bias{at}")) {
                layer.bias.set(bias)?;
            }
        }
        Ok(())
    }

    /// A network with the weights a file holds.
    pub fn from_file(path: &Path, seed: u64) -> Chance<Net> {
        let mut net = Net::fresh(seed)?;
        let held = candle_core::safetensors::load(path, &Device::Cpu)?;
        net.load_from(&held)?;
        Ok(net)
    }

    /// A network with the weights a block of bytes holds.
    pub fn from_bytes(bytes: &[u8], seed: u64) -> Chance<Net> {
        let mut net = Net::fresh(seed)?;
        let held = candle_core::safetensors::load_buffer(bytes, &Device::Cpu)?;
        net.load_from(&held)?;
        Ok(net)
    }
}

/// Adam: what an optimiser remembers between steps.
///
/// Written out rather than taken from a library because it is fifteen lines
/// and because it keeps the seam at one file.
pub struct Adam {
    /// How far a step goes.
    pub rate: f32,
    /// The running average of the gradient, per weight.
    first: Vec<Tensor>,
    /// The running average of its square.
    second: Vec<Tensor>,
    /// How many steps have been taken, for the warm-up correction.
    steps: f32,
}

/// How much of the last average of the gradient is kept.
const KEEP_FIRST: f64 = 0.9;
/// How much of the last average of its square is kept.
const KEEP_SECOND: f64 = 0.999;
/// What keeps a step from dividing by nothing.
const STEADY: f64 = 1e-8;

impl Adam {
    /// An optimiser for one network.
    pub fn new(net: &Net, rate: f32) -> Chance<Adam> {
        let mut first = Vec::new();
        let mut second = Vec::new();
        for weight in net.weights() {
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
    pub fn step(&mut self, net: &Net, loss: &Tensor) -> Chance<()> {
        let grads = loss.backward()?;
        self.steps += 1.0;
        let warm_first = 1.0 - KEEP_FIRST.powf(f64::from(self.steps));
        let warm_second = 1.0 - KEEP_SECOND.powf(f64::from(self.steps));
        for (at, weight) in net.weights().into_iter().enumerate() {
            let Some(grad) = grads.get(weight) else {
                continue;
            };
            let first = ((&self.first[at] * KEEP_FIRST)? + (grad * (1.0 - KEEP_FIRST))?)?;
            let second =
                ((&self.second[at] * KEEP_SECOND)? + (grad.sqr()? * (1.0 - KEEP_SECOND))?)?;
            let step = ((&first / warm_first)? / (((&second / warm_second)?.sqrt()? + STEADY)?))?;
            weight.set(&(weight.as_tensor() - (step * f64::from(self.rate))?)?)?;
            // Cut loose from how they were worked out. Kept attached, each
            // step's averages point at the last step's, and after a few
            // thousand steps letting go of them walks a chain that deep and
            // takes the stack with it.
            self.first[at] = first.detach();
            self.second[at] = second.detach();
        }
        Ok(())
    }
}

/// The loss of choosing one row out of a set, weighted.
///
/// Cross entropy over the scores: the score of the row that was chosen, less
/// what all the rows together come to. Pushing it down pushes the chosen row's
/// score up and the rest down, which is what "prefer this one" means when the
/// answer is a choice rather than a number.
pub fn choice_loss(scores: &Tensor, chosen: usize, weight: f32) -> Chance<Tensor> {
    let highest = scores.max_keepdim(0)?;
    let steady = scores.broadcast_sub(&highest)?;
    let total = steady.exp()?.sum_keepdim(0)?.log()?;
    let mine = steady.narrow(0, chosen, 1)?;
    let loss = (total - mine)?;
    loss.flatten_all()? * f64::from(weight)
}
