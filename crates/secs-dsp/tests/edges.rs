#![forbid(unsafe_code)]
use secs_dsp::{
    iir::{self, BType, Sos},
    optimize::{self, LeastSquaresTrace, Options},
};

#[test]
fn invalid_sos_and_nonfinite_coefficients() {
    assert!(iir::sosfilt(&Sos(vec![]), &[1.]).is_err());
    assert!(iir::sosfilt(&Sos(vec![[1., 0., 0., 1., 0., 0.]]), &[]).is_err());
    assert!(iir::sosfilt(&Sos(vec![[1., 0., 0., 2., 0., 0.]]), &[1.]).is_err());
    assert!(iir::sosfreqz(&Sos(vec![]), &[0.], 48000.).is_err());
    assert!(
        iir::sosfilt(&Sos(vec![[f64::NAN, 0., 0., 1., 0., 0.]]), &[1., 0., 0.])
            .unwrap()
            .iter()
            .all(|x| x.is_nan())
    );
    for cutoff in [0., 24000., f64::NAN, f64::INFINITY] {
        assert!(iir::butter(2, &[cutoff], BType::Lowpass, 48000.).is_err());
    }
}

#[test]
fn trf_validation_and_trace_accounting() {
    assert!(
        optimize::least_squares(
            |x| x.to_vec(),
            &[0.],
            &[f64::NEG_INFINITY],
            &[f64::INFINITY],
            5
        )
        .is_err()
    );
    assert!(optimize::least_squares(|_| vec![f64::NAN], &[0.], &[-1.], &[1.], 5).is_err());
    assert!(optimize::least_squares(|x| x.to_vec(), &[2.], &[-1.], &[1.], 5).is_err());
    let mut trace = LeastSquaresTrace::default();
    let r = optimize::least_squares_with_options(
        |x| vec![x[0] - 0.5],
        &[0.],
        &[-1.],
        &[1.],
        Options {
            max_nfev: 1,
            ..Options::default()
        },
        Some(&mut trace),
    )
    .unwrap();
    assert_eq!((r.nfev, r.njev, r.status), (1, 1, 0));
    assert_eq!(trace.evaluations.len(), 2);
    assert_eq!(trace.evaluations[0].column, None);
    assert_eq!(trace.evaluations[1].column, Some(0));
    assert_eq!(trace.jacobian_points, vec![vec![0.]]);
}
