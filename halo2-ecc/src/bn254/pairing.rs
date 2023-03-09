#![allow(non_snake_case)]
use super::{Fp12Chip, Fp2Chip, FpChip, FpPoint, FqPoint};
use crate::ecc::pairing::PairingChip;
use crate::halo2_proofs::halo2curves::bn256::{
    G1Affine, G2Affine, FROBENIUS_COEFF_FQ12_C1, SIX_U_PLUS_2_NAF,
};
use crate::{
    ecc::{EcPoint, EccChip},
    fields::fp12::mul_no_carry_w6,
    fields::{FieldChip, FieldExtPoint, PrimeField},
};
use halo2_base::Context;
use num_bigint::BigUint;

const XI_0: i64 = 9;

// Inputs:
//  Q0 = (x_1, y_1) and Q1 = (x_2, y_2) are points in E(Fp2)
//  P is point (X, Y) in E(Fp)
// Assuming Q0 != Q1
// Output:
//  line_{Psi(Q0), Psi(Q1)}(P) where Psi(x,y) = (w^2 x, w^3 y)
//  - equals w^3 (y_1 - y_2) X + w^2 (x_2 - x_1) Y + w^5 (x_1 y_2 - x_2 y_1) =: out3 * w^3 + out2 * w^2 + out5 * w^5 where out2, out3, out5 are Fp2 points
// Output is [None, None, out2, out3, None, out5] as vector of `Option<FqPoint>`s
pub fn sparse_line_function_unequal<F: PrimeField>(
    fp2_chip: &Fp2Chip<F>,
    ctx: &mut Context<F>,
    Q: (&EcPoint<F, FqPoint<F>>, &EcPoint<F, FqPoint<F>>),
    P: &EcPoint<F, FpPoint<F>>,
) -> Vec<Option<FqPoint<F>>> {
    let (x_1, y_1) = (&Q.0.x, &Q.0.y);
    let (x_2, y_2) = (&Q.1.x, &Q.1.y);
    let (X, Y) = (&P.x, &P.y);
    assert_eq!(x_1.coeffs.len(), 2);
    assert_eq!(y_1.coeffs.len(), 2);
    assert_eq!(x_2.coeffs.len(), 2);
    assert_eq!(y_2.coeffs.len(), 2);

    let y1_minus_y2 = fp2_chip.sub_no_carry(ctx, y_1, y_2);
    let x2_minus_x1 = fp2_chip.sub_no_carry(ctx, x_2, x_1);
    let x1y2 = fp2_chip.mul_no_carry(ctx, x_1, y_2);
    let x2y1 = fp2_chip.mul_no_carry(ctx, x_2, y_1);

    let out3 = fp2_chip.fp_mul_no_carry(ctx, &y1_minus_y2, X);
    let out2 = fp2_chip.fp_mul_no_carry(ctx, &x2_minus_x1, Y);
    let out5 = fp2_chip.sub_no_carry(ctx, &x1y2, &x2y1);

    // so far we have not "carried mod p" for any of the outputs
    // we do this below
    vec![None, None, Some(out2), Some(out3), None, Some(out5)]
        .iter()
        .map(|option_nc| option_nc.as_ref().map(|nocarry| fp2_chip.carry_mod(ctx, nocarry)))
        .collect()
}

// Assuming curve is of form Y^2 = X^3 + b (a = 0) to save operations
// Inputs:
//  Q = (x, y) is a point in E(Fp2)
//  P = (P.x, P.y) in E(Fp)
// Output:
//  line_{Psi(Q), Psi(Q)}(P) where Psi(x,y) = (w^2 x, w^3 y)
//  - equals (3x^3 - 2y^2)(XI_0 + u) + w^4 (-3 x^2 * Q.x) + w^3 (2 y * Q.y) =: out0 + out4 * w^4 + out3 * w^3 where out0, out3, out4 are Fp2 points
// Output is [out0, None, None, out3, out4, None] as vector of `Option<FqPoint>`s
pub fn sparse_line_function_equal<F: PrimeField>(
    fp2_chip: &Fp2Chip<F>,
    ctx: &mut Context<F>,
    Q: &EcPoint<F, FqPoint<F>>,
    P: &EcPoint<F, FpPoint<F>>,
) -> Vec<Option<FqPoint<F>>> {
    let (x, y) = (&Q.x, &Q.y);
    assert_eq!(x.coeffs.len(), 2);
    assert_eq!(y.coeffs.len(), 2);

    let x_sq = fp2_chip.mul(ctx, x, x);

    let x_cube = fp2_chip.mul_no_carry(ctx, &x_sq, x);
    let three_x_cu = fp2_chip.scalar_mul_no_carry(ctx, &x_cube, 3);
    let y_sq = fp2_chip.mul_no_carry(ctx, y, y);
    let two_y_sq = fp2_chip.scalar_mul_no_carry(ctx, &y_sq, 2);
    let out0_left = fp2_chip.sub_no_carry(ctx, &three_x_cu, &two_y_sq);
    let out0 = mul_no_carry_w6::<F, FpChip<F>, XI_0>(fp2_chip.fp_chip, ctx, &out0_left);

    let x_sq_Px = fp2_chip.fp_mul_no_carry(ctx, &x_sq, &P.x);
    let out4 = fp2_chip.scalar_mul_no_carry(ctx, &x_sq_Px, -3);

    let y_Py = fp2_chip.fp_mul_no_carry(ctx, y, &P.y);
    let out3 = fp2_chip.scalar_mul_no_carry(ctx, &y_Py, 2);

    // so far we have not "carried mod p" for any of the outputs
    // we do this below
    vec![Some(out0), None, None, Some(out3), Some(out4), None]
        .iter()
        .map(|option_nc| option_nc.as_ref().map(|nocarry| fp2_chip.carry_mod(ctx, nocarry)))
        .collect()
}

// multiply Fp12 point `a` with Fp12 point `b` where `b` is len 6 vector of Fp2 points, where some are `None` to represent zero.
// Assumes `b` is not vector of all `None`s
pub fn sparse_fp12_multiply<F: PrimeField>(
    fp2_chip: &Fp2Chip<F>,
    ctx: &mut Context<F>,
    a: &FqPoint<F>,
    b_fp2_coeffs: &Vec<Option<FqPoint<F>>>,
) -> FieldExtPoint<FpPoint<F>> {
    assert_eq!(a.coeffs.len(), 12);
    assert_eq!(b_fp2_coeffs.len(), 6);
    let mut a_fp2_coeffs = Vec::with_capacity(6);
    for i in 0..6 {
        a_fp2_coeffs.push(FqPoint::construct(vec![a.coeffs[i].clone(), a.coeffs[i + 6].clone()]));
    }
    // a * b as element of Fp2[w] without evaluating w^6 = (XI_0 + u)
    let mut prod_2d: Vec<Option<FieldExtPoint<FpPoint<F>>>> = vec![None; 11];
    for i in 0..6 {
        for j in 0..6 {
            prod_2d[i + j] =
                match (prod_2d[i + j].clone(), &a_fp2_coeffs[i], b_fp2_coeffs[j].as_ref()) {
                    (a, _, None) => a,
                    (None, a, Some(b)) => {
                        let ab = fp2_chip.mul_no_carry(ctx, a, b);
                        Some(ab)
                    }
                    (Some(a), b, Some(c)) => {
                        let bc = fp2_chip.mul_no_carry(ctx, b, c);
                        let out = fp2_chip.add_no_carry(ctx, &a, &bc);
                        Some(out)
                    }
                };
        }
    }

    let mut out_fp2 = Vec::with_capacity(6);
    for i in 0..6 {
        // prod_2d[i] + prod_2d[i+6] * w^6
        let prod_nocarry = if i != 5 {
            let eval_w6 = prod_2d[i + 6]
                .as_ref()
                .map(|a| mul_no_carry_w6::<F, FpChip<F>, XI_0>(fp2_chip.fp_chip, ctx, a));
            match (prod_2d[i].as_ref(), eval_w6) {
                (None, b) => b.unwrap(), // Our current use cases of 235 and 034 sparse multiplication always result in non-None value
                (Some(a), None) => a.clone(),
                (Some(a), Some(b)) => fp2_chip.add_no_carry(ctx, a, &b),
            }
        } else {
            prod_2d[i].clone().unwrap()
        };
        let prod = fp2_chip.carry_mod(ctx, &prod_nocarry);
        out_fp2.push(prod);
    }

    let mut out_coeffs = Vec::with_capacity(12);
    for fp2_coeff in &out_fp2 {
        out_coeffs.push(fp2_coeff.coeffs[0].clone());
    }
    for fp2_coeff in &out_fp2 {
        out_coeffs.push(fp2_coeff.coeffs[1].clone());
    }
    FqPoint::construct(out_coeffs)
}

// Input:
// - g is Fp12 point
// - Q = (P0, P1) with Q0, Q1 points in E(Fp2)
// - P is point in E(Fp)
// Output:
// - out = g * l_{Psi(Q0), Psi(Q1)}(P) as Fp12 point
pub fn fp12_multiply_with_line_unequal<F: PrimeField>(
    fp2_chip: &Fp2Chip<F>,
    ctx: &mut Context<F>,
    g: &FqPoint<F>,
    Q: (&EcPoint<F, FqPoint<F>>, &EcPoint<F, FqPoint<F>>),
    P: &EcPoint<F, FpPoint<F>>,
) -> FqPoint<F> {
    let line = sparse_line_function_unequal::<F>(fp2_chip, ctx, Q, P);
    sparse_fp12_multiply::<F>(fp2_chip, ctx, g, &line)
}

// Input:
// - g is Fp12 point
// - Q is point in E(Fp2)
// - P is point in E(Fp)
// Output:
// - out = g * l_{Psi(Q), Psi(Q)}(P) as Fp12 point
pub fn fp12_multiply_with_line_equal<F: PrimeField>(
    fp2_chip: &Fp2Chip<F>,
    ctx: &mut Context<F>,
    g: &FqPoint<F>,
    Q: &EcPoint<F, FqPoint<F>>,
    P: &EcPoint<F, FpPoint<F>>,
) -> FqPoint<F> {
    let line = sparse_line_function_equal::<F>(fp2_chip, ctx, Q, P);
    sparse_fp12_multiply::<F>(fp2_chip, ctx, g, &line)
}

// Assuming curve is of form `y^2 = x^3 + b` for now (a = 0) for less operations
// Value of `b` is never used
// Inputs:
// - pairs: vec of (P, Q) pairs, where
//     - P is a point in E(Fp)
//     - Q = (x, y) is a point in E(Fp2)
// - `pseudo_binary_encoding` is fixed vector consisting of {-1, 0, 1} entries such that `loop_count = sum pseudo_binary_encoding[i] * 2^i`
// Output:
//  - Prod_i e'(P_i, Q_i), where e'(P_i, Q_i) is:
//      e'(P, Q) = f_{loop_count}(P, Q) * l_{[loop_count] Q', Frob_p(Q')}(P) * l_{[loop_count] Q' + Frob_p(Q'), -Frob_p^2(Q')}(P)
//  - where we start with `f_1(Q,P) = 1` and use Miller's algorithm f_{i+j} = f_i * f_j * l_{i,j}(Q,P)
//  - Q' = Psi(Q) in E(Fp12)
//  - Frob_p(x,y) = (x^p, y^p)
//  - Above formula is specific to BN curves
// Assume:
//  - Q != O and the order of Q in E(Fp2) is r
//  - r is prime, so [i]Q != [j]Q for i != j in Z/r
//  - `0 <= loop_count < r` and `loop_count < p` (to avoid [loop_count]Q' = Frob_p(Q'))
//  - x^3 + b = 0 has no solution in Fp2, i.e., the y-coordinate of Q cannot be 0.
pub fn multi_miller_loop_BN<F: PrimeField>(
    ecc_chip: &EccChip<F, Fp2Chip<F>>,
    ctx: &mut Context<F>,
    pairs: Vec<(&EcPoint<F, FpPoint<F>>, &EcPoint<F, FqPoint<F>>)>,
    pseudo_binary_encoding: &[i8],
) -> FqPoint<F> {
    let mut i = pseudo_binary_encoding.len() - 1;
    while pseudo_binary_encoding[i] == 0 {
        i -= 1;
    }
    let last_index = i;
    assert_eq!(pseudo_binary_encoding[last_index], 1);

    let neg_b = pairs.iter().map(|pair| ecc_chip.negate(ctx, pair.1)).collect::<Vec<_>>();

    // initialize the first line function into Fq12 point
    let mut f = {
        let sparse_f =
            sparse_line_function_equal::<F>(ecc_chip.field_chip(), ctx, pairs[0].1, pairs[0].0);
        assert_eq!(sparse_f.len(), 6);

        let zero_fp = ecc_chip.field_chip.fp_chip.load_constant(ctx, BigUint::from(0u64));
        let mut f_coeffs = Vec::with_capacity(12);
        for coeff in &sparse_f {
            if let Some(fp2_point) = coeff {
                f_coeffs.push(fp2_point.coeffs[0].clone());
            } else {
                f_coeffs.push(zero_fp.clone());
            }
        }
        for coeff in &sparse_f {
            if let Some(fp2_point) = coeff {
                f_coeffs.push(fp2_point.coeffs[1].clone());
            } else {
                f_coeffs.push(zero_fp.clone());
            }
        }
        FqPoint::construct(f_coeffs)
    };
    for &(a, b) in pairs.iter().skip(1) {
        f = fp12_multiply_with_line_equal::<F>(ecc_chip.field_chip(), ctx, &f, b, a);
    }

    i -= 1;
    let mut r = pairs.iter().map(|pair| pair.1.clone()).collect::<Vec<_>>();
    let fp12_chip = Fp12Chip::<F>::new(ecc_chip.field_chip.fp_chip);
    loop {
        if i != last_index - 1 {
            f = fp12_chip.mul(ctx, &f, &f);
            for (r, &(a, _)) in r.iter().zip(pairs.iter()) {
                f = fp12_multiply_with_line_equal::<F>(ecc_chip.field_chip(), ctx, &f, r, a);
            }
        }
        for r in r.iter_mut() {
            *r = ecc_chip.double(ctx, r);
        }

        assert!(pseudo_binary_encoding[i] <= 1 && pseudo_binary_encoding[i] >= -1);
        if pseudo_binary_encoding[i] != 0 {
            for ((r, neg_b), &(a, b)) in r.iter_mut().zip(neg_b.iter()).zip(pairs.iter()) {
                let sign_b = if pseudo_binary_encoding[i] == 1 { b } else { neg_b };
                f = fp12_multiply_with_line_unequal::<F>(
                    ecc_chip.field_chip(),
                    ctx,
                    &f,
                    (r, sign_b),
                    a,
                );
                *r = ecc_chip.add_unequal(ctx, r, sign_b, false);
            }
        }
        if i == 0 {
            break;
        }
        i -= 1;
    }

    // Frobenius coefficient coeff[1][j] = ((9+u)^{(p-1)/6})^j
    // load coeff[1][2], coeff[1][3]
    let c2 = FROBENIUS_COEFF_FQ12_C1[1] * FROBENIUS_COEFF_FQ12_C1[1];
    let c3 = c2 * FROBENIUS_COEFF_FQ12_C1[1];
    let c2 = ecc_chip.field_chip.load_constant(ctx, c2);
    let c3 = ecc_chip.field_chip.load_constant(ctx, c3);

    // finish multiplying remaining line functions outside the loop
    for (r, &(a, b)) in r.iter_mut().zip(pairs.iter()) {
        let b_1 = twisted_frobenius::<F>(ecc_chip, ctx, b, &c2, &c3);
        let neg_b_2 = neg_twisted_frobenius::<F>(ecc_chip, ctx, &b_1, &c2, &c3);
        f = fp12_multiply_with_line_unequal::<F>(ecc_chip.field_chip(), ctx, &f, (r, &b_1), a);
        *r = ecc_chip.add_unequal(ctx, r, &b_1, false);
        f = fp12_multiply_with_line_unequal::<F>(ecc_chip.field_chip(), ctx, &f, (r, &neg_b_2), a);
    }
    f
}

// Frobenius coefficient coeff[1][j] = ((9+u)^{(p-1)/6})^j
// Frob_p( twist(Q) ) = ( (w^2 x)^p, (w^3 y)^p ) = twist( coeff[1][2] * x^p, coeff[1][3] * y^p )
// Input:
// - Q = (x, y) point in E(Fp2)
// - coeff[1][2], coeff[1][3] as assigned cells: this is an optimization to avoid loading new constants
// Output:
// - (coeff[1][2] * x^p, coeff[1][3] * y^p) point in E(Fp2)
pub fn twisted_frobenius<F: PrimeField>(
    ecc_chip: &EccChip<F, Fp2Chip<F>>,
    ctx: &mut Context<F>,
    Q: &EcPoint<F, FqPoint<F>>,
    c2: &FqPoint<F>,
    c3: &FqPoint<F>,
) -> EcPoint<F, FqPoint<F>> {
    assert_eq!(c2.coeffs.len(), 2);
    assert_eq!(c3.coeffs.len(), 2);

    let frob_x = ecc_chip.field_chip.conjugate(ctx, &Q.x);
    let frob_y = ecc_chip.field_chip.conjugate(ctx, &Q.y);
    let out_x = ecc_chip.field_chip.mul(ctx, c2, &frob_x);
    let out_y = ecc_chip.field_chip.mul(ctx, c3, &frob_y);
    EcPoint::construct(out_x, out_y)
}

// Frobenius coefficient coeff[1][j] = ((9+u)^{(p-1)/6})^j
// -Frob_p( twist(Q) ) = ( (w^2 x)^p, -(w^3 y)^p ) = twist( coeff[1][2] * x^p, coeff[1][3] * -y^p )
// Input:
// - Q = (x, y) point in E(Fp2)
// Output:
// - (coeff[1][2] * x^p, coeff[1][3] * -y^p) point in E(Fp2)
pub fn neg_twisted_frobenius<F: PrimeField>(
    ecc_chip: &EccChip<F, Fp2Chip<F>>,
    ctx: &mut Context<F>,
    Q: &EcPoint<F, FqPoint<F>>,
    c2: &FqPoint<F>,
    c3: &FqPoint<F>,
) -> EcPoint<F, FqPoint<F>> {
    assert_eq!(c2.coeffs.len(), 2);
    assert_eq!(c3.coeffs.len(), 2);

    let frob_x = ecc_chip.field_chip.conjugate(ctx, &Q.x);
    let neg_frob_y = ecc_chip.field_chip.neg_conjugate(ctx, &Q.y);
    let out_x = ecc_chip.field_chip.mul(ctx, c2, &frob_x);
    let out_y = ecc_chip.field_chip.mul(ctx, c3, &neg_frob_y);
    EcPoint::construct(out_x, out_y)
}

// To avoid issues with mutably borrowing twice (not allowed in Rust), we only store fp_chip and construct g2_chip and fp12_chip in scope when needed for temporary mutable borrows
pub struct BN254PairingChip<'chip, F: PrimeField> {
    pub fp_chip: &'chip FpChip<'chip, F>,
}

impl<'chip, F: PrimeField> BN254PairingChip<'chip, F> {
    pub fn new(fp_chip: &'chip FpChip<F>) -> Self {
        Self { fp_chip }
    }

    pub fn load_private_g1(&self, ctx: &mut Context<F>, point: G1Affine) -> EcPoint<F, FpPoint<F>> {
        let g1_chip = EccChip::new(self.fp_chip);
        g1_chip.load_private(ctx, (point.x, point.y))
    }

    pub fn load_private_g2(
        &self,
        ctx: &mut Context<F>,
        point: G2Affine,
    ) -> EcPoint<F, FieldExtPoint<FpPoint<F>>> {
        let fp2_chip = Fp2Chip::<F>::new(self.fp_chip);
        let g2_chip = EccChip::new(&fp2_chip);
        g2_chip.load_private(ctx, (point.x, point.y))
    }

    pub fn miller_loop(
        &self,
        ctx: &mut Context<F>,
        Q: &EcPoint<F, FqPoint<F>>,
        P: &EcPoint<F, FpPoint<F>>,
    ) -> FqPoint<F> {
        let pair = vec![(P, Q)];
        self.multi_miller_loop(ctx, pair)
    }
}

impl<'chip, F: PrimeField> PairingChip<F> for BN254PairingChip<'chip, F> {
    fn multi_miller_loop(
        &self,
        ctx: &mut Context<F>,
        pairs: Vec<(
            &EcPoint<F, crate::ecc::pairing::FpPoint<F>>,
            &EcPoint<F, crate::ecc::pairing::FqPoint<F>>,
        )>,
    ) -> FieldExtPoint<crate::bigint::CRTInteger<F>> {
        let fp2_chip = Fp2Chip::<F>::new(self.fp_chip);
        let g2_chip = EccChip::new(&fp2_chip);
        multi_miller_loop_BN::<F>(
            &g2_chip,
            ctx,
            pairs,
            &SIX_U_PLUS_2_NAF, // pseudo binary encoding for BN254
        )
    }

    fn final_exp(
        &self,
        ctx: &mut Context<F>,
        f: &crate::ecc::pairing::FqPoint<F>,
    ) -> crate::ecc::pairing::FqPoint<F> {
        let fp12_chip = Fp12Chip::<F>::new(self.fp_chip);
        fp12_chip.final_exp(ctx, f)
    }
}
