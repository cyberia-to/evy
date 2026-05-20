//! Goldilocks (de)serialization for primitive types.
//!
//! `GoldilocksCodec` is the bridge between Rust types and the field-element
//! representation `ShardStore` stores. One Goldilocks element = u64
//! (Goldilocks field p = 2^64 - 2^32 + 1, fits a full u64 minus a sliver
//! near the top).
//!
//! For values that comfortably fit in u64 the encoding is bit-cast.
//! For floats we bit-cast through their integer width. Composite types
//! (Vec3, Quat) chain their fields.
//!
//! `EvyComponent` impls call `GoldilocksCodec::encode`/`decode` on their
//! fields; the encoding for a whole component is a concatenation of its
//! fields' encodings.

use nebu::Goldilocks;

/// Encode and decode a value into a field-element representation.
///
/// Goldilocks values are u64 mod p where p = 2^64 - 2^32 + 1. Almost all
/// u64 values fit; values in `[p, u64::MAX]` (~4 billion of them, the
/// top sliver) are not representable. Callers passing arbitrary u64 must
/// reduce; `f32`/`f64`/`u32` are always safe.
pub trait GoldilocksCodec: Sized {
    /// Number of Goldilocks elements this type occupies.
    const WIDTH: usize;

    /// Append this value's encoding to `out`.
    fn encode_into(&self, out: &mut Vec<Goldilocks>);

    /// Decode from a slice. Caller must ensure `slice.len() >= Self::WIDTH`.
    fn decode_from(slice: &[Goldilocks]) -> Self;

    /// Convenience: encode to a fresh Vec.
    fn encode(&self) -> Vec<Goldilocks> {
        let mut v = Vec::with_capacity(Self::WIDTH);
        self.encode_into(&mut v);
        v
    }
}

impl GoldilocksCodec for u32 {
    const WIDTH: usize = 1;
    fn encode_into(&self, out: &mut Vec<Goldilocks>) {
        out.push(Goldilocks::new(*self as u64));
    }
    fn decode_from(slice: &[Goldilocks]) -> Self {
        slice[0].as_u64() as u32
    }
}

impl GoldilocksCodec for u64 {
    const WIDTH: usize = 1;
    fn encode_into(&self, out: &mut Vec<Goldilocks>) {
        // Caller responsibility: must already be < Goldilocks::MODULUS.
        out.push(Goldilocks::new(*self));
    }
    fn decode_from(slice: &[Goldilocks]) -> Self {
        slice[0].as_u64()
    }
}

impl GoldilocksCodec for i32 {
    const WIDTH: usize = 1;
    fn encode_into(&self, out: &mut Vec<Goldilocks>) {
        out.push(Goldilocks::new(*self as u32 as u64));
    }
    fn decode_from(slice: &[Goldilocks]) -> Self {
        slice[0].as_u64() as u32 as i32
    }
}

impl GoldilocksCodec for f32 {
    const WIDTH: usize = 1;
    fn encode_into(&self, out: &mut Vec<Goldilocks>) {
        out.push(Goldilocks::new(self.to_bits() as u64));
    }
    fn decode_from(slice: &[Goldilocks]) -> Self {
        f32::from_bits(slice[0].as_u64() as u32)
    }
}

impl GoldilocksCodec for f64 {
    const WIDTH: usize = 1;
    fn encode_into(&self, out: &mut Vec<Goldilocks>) {
        // f64 → u64 bits. Top sliver above Goldilocks modulus is the
        // exponent-all-ones region (NaN/Inf with high mantissa bits). For
        // game state we typically don't store those; if needed, encode as
        // two u32 halves instead.
        let bits = self.to_bits();
        out.push(Goldilocks::new(bits));
    }
    fn decode_from(slice: &[Goldilocks]) -> Self {
        f64::from_bits(slice[0].as_u64())
    }
}

impl GoldilocksCodec for bool {
    const WIDTH: usize = 1;
    fn encode_into(&self, out: &mut Vec<Goldilocks>) {
        out.push(Goldilocks::new(if *self { 1 } else { 0 }));
    }
    fn decode_from(slice: &[Goldilocks]) -> Self {
        slice[0].as_u64() != 0
    }
}

impl<const N: usize, T: GoldilocksCodec + Copy> GoldilocksCodec for [T; N] {
    const WIDTH: usize = N * T::WIDTH;
    fn encode_into(&self, out: &mut Vec<Goldilocks>) {
        for x in self {
            x.encode_into(out);
        }
    }
    fn decode_from(slice: &[Goldilocks]) -> Self {
        let mut arr: [core::mem::MaybeUninit<T>; N] =
            unsafe { core::mem::MaybeUninit::uninit().assume_init() };
        for i in 0..N {
            arr[i].write(T::decode_from(&slice[i * T::WIDTH..(i + 1) * T::WIDTH]));
        }
        // SAFETY: all elements initialized above.
        arr.map(|x| unsafe { x.assume_init() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u32_round_trip() {
        let original = 0xDEADBEEFu32;
        let encoded = original.encode();
        assert_eq!(encoded.len(), 1);
        assert_eq!(u32::decode_from(&encoded), original);
    }

    #[test]
    fn f32_round_trip() {
        for &v in &[0.0f32, 1.0, -1.0, 3.14, f32::MIN, f32::MAX] {
            let enc = v.encode();
            assert_eq!(f32::decode_from(&enc).to_bits(), v.to_bits());
        }
    }

    #[test]
    fn f64_round_trip_for_finite() {
        for &v in &[0.0f64, 1.0, -1.0, 3.14159265358979] {
            let enc = v.encode();
            assert_eq!(f64::decode_from(&enc), v);
        }
    }

    #[test]
    fn array_round_trip() {
        let original: [f32; 3] = [1.5, 2.5, 3.5];
        let encoded = original.encode();
        assert_eq!(encoded.len(), 3);
        let decoded: [f32; 3] = <[f32; 3]>::decode_from(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn array_of_u32() {
        let original: [u32; 4] = [1, 2, 3, 4];
        let encoded = original.encode();
        let decoded = <[u32; 4]>::decode_from(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn nested_array() {
        let original: [[f32; 3]; 4] = [
            [1.0, 2.0, 3.0],
            [4.0, 5.0, 6.0],
            [7.0, 8.0, 9.0],
            [10.0, 11.0, 12.0],
        ];
        let encoded = original.encode();
        assert_eq!(encoded.len(), 12);
        let decoded = <[[f32; 3]; 4]>::decode_from(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn bool_round_trip() {
        assert_eq!(bool::decode_from(&true.encode()), true);
        assert_eq!(bool::decode_from(&false.encode()), false);
    }
}
