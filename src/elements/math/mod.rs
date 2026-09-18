//! Math handling for Track D.
//!
//! Three tiers are planned:
//! * `math-base` (default ON) — pure-Rust analyzers + raw-LaTeX wrapper.
//!   Pulls annotations off `MathML` / `KaTeX` and rewrites to `<span data-math>`.
//! * `math-mathml-to-latex` (future) — adds MathML→LaTeX conversion.
//! * `math-full` (future) — `math-base` + both converters.
//!
//! Only `base` is implemented in this track.

#[cfg(feature = "math-base")]
pub mod base;

#[cfg(feature = "math-base")]
pub use base::normalize_math_base;
