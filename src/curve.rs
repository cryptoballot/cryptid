use crate::base64_serde;
use std::iter::Sum;
use std::ops::{Add, Sub};

use curve25519_dalek::ristretto::{RistrettoPoint, CompressedRistretto};
use curve25519_dalek::traits::Identity;
use num_bigint::BigUint;

use crate::{CryptoError, Scalar, DalekScalar};
use crate::elgamal::CryptoContext;
use crate::util::AsBase64;
use std::convert::TryFrom;

const K: u32 = 12;

pub fn scalar_max_size_bytes() -> usize {
    ((252 - K) / 8) as usize
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct CurveElem(RistrettoPoint);

impl CurveElem {
    pub fn identity() -> Self {
        Self(RistrettoPoint::identity())
    }

    pub fn as_bytes(&self) -> [u8; 32] {
        *self.0.compress().as_bytes()
    }

    pub fn decoded(&self) -> Result<Scalar, CryptoError> {
        let adjusted = Scalar::from(self.0.compress().to_bytes());
        let x = BigUint::from_bytes_le(adjusted.as_bytes()) / 2u32.pow(K);
        if x.bits() as usize > scalar_max_size_bytes() * 8 + K as usize + 4 {
            Err(CryptoError::Decoding)
        } else {
            Ok(x.into())
        }
    }

    pub fn scaled(&self, other: &Scalar) -> Self {
        Self(other.0 * self.0)
    }

    pub fn generator() -> Self {
        Self(curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT)
    }
}

impl AsBase64 for CurveElem {
    type Error = CryptoError;

    fn as_base64(&self) -> String {
            base64::encode(&self.as_bytes())
        }

    fn try_from_base64(encoded: &str) -> Result<Self, Self::Error> {
        let decoded = base64::decode(encoded).map_err(|_| CryptoError::Decoding)?;
        if decoded.len() == 32 {
            Ok(Self(CompressedRistretto::from_slice(&decoded).decompress().ok_or(CryptoError::Decoding)?))
        } else {
            Err(CryptoError::Decoding)
        }
    }
}

base64_serde!(crate::curve::CurveElem);

impl Add for CurveElem {
    type Output = CurveElem;

    fn add(self, rhs: Self) -> Self::Output {
        CurveElem(self.0 + rhs.0)
    }
}

impl Sub for CurveElem {
    type Output = CurveElem;

    fn sub(self, rhs: Self) -> Self::Output {
        CurveElem(self.0 - rhs.0)
    }
}

impl Add for &CurveElem {
    type Output = CurveElem;

    fn add(self, rhs: Self) -> Self::Output {
        CurveElem(self.0 + rhs.0)
    }
}

impl Sub for &CurveElem {
    type Output = CurveElem;

    fn sub(self, rhs: Self) -> Self::Output {
        CurveElem(self.0 - rhs.0)
    }
}

impl Sum for CurveElem {
    fn sum<I: Iterator<Item=Self>>(iter: I) -> Self {
        iter.fold(Self::identity(), |acc, x| acc + x)
    }
}

impl TryFrom<Scalar> for CurveElem {
    type Error = CryptoError;

    fn try_from(s: Scalar) -> Result<Self, CryptoError> {
        // Can encode at most 252 - K bits
        let x: BigUint = s.clone().into();
        let bits = x.bits() as usize;

        let mut s = s.0;
        if bits > (252 - K) as usize {
            return Err(CryptoError::Encoding);
        }

        let buffer = DalekScalar::from(2u32.pow(K));
        s *= buffer;
        let mut d = DalekScalar::zero();
        loop {
            if let Some(p) = CompressedRistretto((s + d).to_bytes()).decompress() {
                return Ok(Self(p));
            }

            d += DalekScalar::one();
            if d - buffer == DalekScalar::zero() {
                return Err(CryptoError::Encoding);
            }
        }
    }
}

impl TryFrom<BigUint> for CurveElem {
    type Error = CryptoError;

    fn try_from(n: BigUint) -> Result<Self, CryptoError> {
        let s: Scalar = n.into();
        Self::try_from(s)
    }
}

impl TryFrom<&[u8]> for CurveElem {
    type Error = CryptoError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::try_from(BigUint::from_bytes_be(value))
    }
}

impl TryFrom<u32> for CurveElem {
    type Error = CryptoError;

    fn try_from(n: u32) -> Result<Self, CryptoError> {
        Self::try_from(BigUint::from(n))
    }
}

impl TryFrom<u64> for CurveElem {
    type Error = CryptoError;

    fn try_from(n: u64) -> Result<Self, CryptoError> {
        Self::try_from(BigUint::from(n))
    }
}

#[derive(Debug)]
pub struct Polynomial {
    k: usize,
    n: usize,
    pub x_i: Scalar,
    ctx: CryptoContext,
    coefficients: Vec<DalekScalar>,
}

impl Polynomial {
    pub fn random(ctx: &mut CryptoContext, k: usize, n: usize) -> Result<Polynomial, CryptoError> {
        let mut ctx = ctx.clone();
        let x_i = ctx.random_power()?;
        let mut coefficients = Vec::with_capacity(k);
        coefficients.push(x_i.0);
        for _ in 1..k {
            coefficients.push(ctx.random_power()?.0);
        }

        Ok(Polynomial { k, n, x_i, ctx, coefficients })
    }

    pub fn get_public_params(&self) -> Vec<CurveElem> {
        self.coefficients.iter()
            .map(|coeff| self.ctx.g_to(&Scalar(coeff.clone())))
            .collect()
    }

    pub fn evaluate(&self, i: u32) -> Scalar {
        Scalar((0..self.k).map(|l| {
            self.coefficients[l] * DalekScalar::from(i.pow(l as u32))
        }).sum())
    }
}

#[cfg(test)]
mod tests {
    use crate::elgamal;

    #[test]
    fn test_curveelem_serde() {
        let mut ctx = elgamal::CryptoContext::new();
        let s = ctx.random_power().unwrap();
        let elem = ctx.g_to(&s);

        let encoded = serde_json::to_string(&elem).unwrap();
        let decoded = serde_json::from_str(&encoded).unwrap();
        assert_eq!(elem, decoded);
    }
}