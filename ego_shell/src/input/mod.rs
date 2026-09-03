pub mod editor;
pub mod normalize;
pub use editor::{EditorAction, LineEditor};
pub use normalize::{NormalizedKey, InputNormalizer, Normalizer};