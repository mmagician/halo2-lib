use halo2_base::Context;

use crate::fields::PrimeField;
use crate::halo2_proofs::halo2curves::bn256::{Fq, Fq12, Fq2};
use crate::{
    bigint::CRTInteger,
    fields::{fp, fp12, fp2, FieldExtPoint},
};

use super::EcPoint;

pub type FpChip<'range, F> = fp::FpChip<'range, F, Fq>;
pub type FpPoint<F> = CRTInteger<F>;
pub type FqPoint<F> = FieldExtPoint<FpPoint<F>>;
pub type Fp2Chip<'chip, F> = fp2::Fp2Chip<'chip, F, FpChip<'chip, F>, Fq2>;
pub type Fp12Chip<'chip, F> = fp12::Fp12Chip<'chip, F, FpChip<'chip, F>, Fq12, 9>;

pub trait PairingChip<F: PrimeField> {
    fn miller_loop(
        &self,
        ctx: &mut Context<F>,
        Q: &EcPoint<F, FieldExtPoint<CRTInteger<F>>>,
        P: &EcPoint<F, CRTInteger<F>>,
    ) -> FieldExtPoint<CRTInteger<F>> {
        let pair = vec![(P, Q)];
        self.multi_miller_loop(ctx, pair)
    }

    fn multi_miller_loop(
        &self,
        ctx: &mut Context<F>,
        pairs: Vec<(&EcPoint<F, FpPoint<F>>, &EcPoint<F, FqPoint<F>>)>,
    ) -> FieldExtPoint<CRTInteger<F>>;

    fn final_exp(&self, ctx: &mut Context<F>, f: &FqPoint<F>) -> FqPoint<F>;

    fn pairing(
        &self,
        ctx: &mut Context<F>,
        Q: &EcPoint<F, FieldExtPoint<CRTInteger<F>>>,
        P: &EcPoint<F, CRTInteger<F>>,
    ) -> FieldExtPoint<CRTInteger<F>> {
        let f0 = self.miller_loop(ctx, Q, P);
        self.final_exp(ctx, &f0)
    }
}
