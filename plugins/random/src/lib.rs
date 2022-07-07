use gal_bindings::*;
use rand::{prelude::StdRng, Rng, SeedableRng};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

lazy_static::lazy_static! {
    static ref RNG: Mutex<StdRng> = Mutex::new(StdRng::seed_from_u64(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    ));
}

export!(rnd);

fn rnd(args: Vec<RawValue>) -> RawValue {
    if let Ok(mut rng) = RNG.lock() {
        let res = match args.len() {
            0 => rng.gen(),
            1 => rng.gen_range(0..args[0].get_num()),
            _ => rng.gen_range(args[0].get_num()..args[1].get_num()),
        };
        RawValue::Num(res)
    } else {
        RawValue::Unit
    }
}
