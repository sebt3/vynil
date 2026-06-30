use crate::{Error, Result};
use rand::{
    rng,
    seq::{IndexedRandom, SliceRandom},
};
use rhai::{Engine, Map};

const LOWER: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u',
    'v', 'w', 'x', 'y', 'z',
];
const UPPER: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U',
    'V', 'W', 'X', 'Y', 'Z',
];
const DIGITS: &[char] = &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
const SYMBOLS: &[char] = &['!', '#', '%', '*', '+', '-', '.', ':', '=', '?', '@', '_'];

pub fn generate(length: usize, lower: usize, upper: usize, digits: usize, symbols: usize) -> Result<String> {
    let classes: [(&[char], usize); 4] = [
        (LOWER, lower),
        (UPPER, upper),
        (DIGITS, digits),
        (SYMBOLS, symbols),
    ];
    let total_min: usize = classes.iter().map(|(_, m)| *m).sum();
    if total_min > length {
        return Err(Error::PasswordSpec(format!(
            "PWD-SPEC-001 sum of class minimums ({total_min}) exceeds requested length ({length})"
        )));
    }
    let pool: Vec<char> = classes
        .iter()
        .filter(|(_, m)| *m > 0)
        .flat_map(|(set, _)| set.iter().copied())
        .collect();
    if pool.is_empty() {
        return Err(Error::PasswordSpec(
            "PWD-SPEC-002 at least one character class must be enabled".into(),
        ));
    }
    let mut rng = rng();
    let mut chars: Vec<char> = Vec::with_capacity(length);
    for (set, m) in &classes {
        for _ in 0..*m {
            chars.push(*set.choose(&mut rng).expect("character class is never empty"));
        }
    }
    while chars.len() < length {
        chars.push(*pool.choose(&mut rng).expect("pool is never empty"));
    }
    chars.shuffle(&mut rng);
    Ok(chars.into_iter().collect())
}

fn class_min(spec: &Map, key: &str) -> usize {
    spec.get(key)
        .and_then(|v| v.as_int().ok())
        .map(|i| i.max(0) as usize)
        .unwrap_or(1)
}

pub fn password_rhai_register(engine: &mut Engine) {
    engine
        .register_fn("gen_password", |len: i64| -> crate::RhaiRes<String> {
            generate(len.max(0) as usize, 1, 1, 1, 1).map_err(|e| format!("{e}").into())
        })
        .register_fn("gen_password", |len: i64, spec: Map| -> crate::RhaiRes<String> {
            generate(
                len.max(0) as usize,
                class_min(&spec, "lower"),
                class_min(&spec, "upper"),
                class_min(&spec, "digits"),
                class_min(&spec, "symbols"),
            )
            .map_err(|e| format!("{e}").into())
        })
        .register_fn("gen_password_alphanum", |len: i64| -> crate::RhaiRes<String> {
            generate(len.max(0) as usize, 1, 1, 1, 0).map_err(|e| format!("{e}").into())
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(s: &str, f: impl Fn(char) -> bool) -> usize {
        s.chars().filter(|c| f(*c)).count()
    }

    #[test]
    fn length_is_respected() {
        assert_eq!(generate(24, 1, 1, 1, 1).unwrap().chars().count(), 24);
    }

    #[test]
    fn guarantees_minimum_per_class() {
        let p = generate(32, 3, 2, 4, 5).unwrap();
        assert!(count(&p, |c| c.is_ascii_lowercase()) >= 3);
        assert!(count(&p, |c| c.is_ascii_uppercase()) >= 2);
        assert!(count(&p, |c| c.is_ascii_digit()) >= 4);
        assert!(count(&p, |c| SYMBOLS.contains(&c)) >= 5);
    }

    #[test]
    fn symbols_zero_yields_alphanumeric() {
        let p = generate(40, 1, 1, 1, 0).unwrap();
        assert!(p.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn single_class_only() {
        let p = generate(16, 0, 0, 1, 0).unwrap();
        assert!(p.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn symbol_set_is_config_safe() {
        let p = generate(200, 1, 1, 1, 50).unwrap();
        for bad in ['"', '\'', '\\', '`', '$'] {
            assert!(!p.contains(bad), "unsafe symbol {bad} leaked into password");
        }
    }

    #[test]
    fn errors_when_minimums_exceed_length() {
        assert!(generate(3, 1, 1, 1, 1).is_err());
    }

    #[test]
    fn errors_when_no_class_enabled() {
        assert!(generate(10, 0, 0, 0, 0).is_err());
    }

    #[test]
    fn two_passwords_differ() {
        assert_ne!(
            generate(24, 1, 1, 1, 1).unwrap(),
            generate(24, 1, 1, 1, 1).unwrap()
        );
    }
}
