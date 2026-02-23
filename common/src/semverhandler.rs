use crate::{Error, Result, RhaiRes, rhai_err};
use rhai::Engine;
use semver::{Prerelease, Version};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Semver {
    pub version: Version,
    pub use_v: bool,
}
// TODO: support des rc (release candidate)
impl Semver {
    pub fn parse(str: &str) -> Result<Self> {
        let use_v = str.starts_with("v");
        let version = if use_v {
            let mut chars = str.chars();
            chars.next();
            Version::parse(chars.as_str()).map_err(Error::Semver)?
        } else {
            Version::parse(str).map_err(Error::Semver)?
        };
        Ok(Self { version, use_v })
    }

    pub fn opt_parse(str: &str) -> Option<Self> {
        match Self::parse(str) {
            Ok(version) => Some(version),
            Err(_) => None,
        }
    }

    pub fn rhai_parse(str: &str) -> RhaiRes<Self> {
        Self::parse(str).map_err(rhai_err)
    }

    pub fn inc_major(&mut self) {
        self.version.major += 1;
        self.version.minor = 0;
        self.version.patch = 0;
        self.version.pre = Prerelease::EMPTY;
    }

    pub fn inc_minor(&mut self) {
        self.version.minor += 1;
        self.version.patch = 0;
        self.version.pre = Prerelease::EMPTY;
    }

    pub fn inc_patch(&mut self) {
        if self.version.pre.is_empty() {
            self.version.patch += 1;
        } else {
            self.version.pre = Prerelease::EMPTY;
        }
    }

    pub fn inc_beta(&mut self) -> Result<()> {
        if self.version.pre.is_empty() || !self.version.pre.starts_with("beta.") {
            self.version.patch += 1;
            self.version.pre = Prerelease::new("beta.1").map_err(|e: semver::Error| Error::Semver(e))?;
        } else {
            let str = self.version.pre.strip_prefix("beta.").unwrap().to_string();
            let beta = str.parse::<u32>().unwrap() + 1;
            self.version.pre =
                Prerelease::new(&format!("beta.{beta}")).map_err(|e: semver::Error| Error::Semver(e))?;
        }
        Ok(())
    }

    pub fn rhai_inc_beta(&mut self) -> RhaiRes<()> {
        self.inc_beta().map_err(rhai_err)
    }

    pub fn inc_alpha(&mut self) -> Result<()> {
        if self.version.pre.is_empty() || !self.version.pre.starts_with("alpha.") {
            self.version.patch += 1;
            self.version.pre = Prerelease::new("alpha.1").map_err(|e: semver::Error| Error::Semver(e))?;
        } else {
            let str = self.version.pre.strip_prefix("alpha.").unwrap().to_string();
            let alpha = str.parse::<u32>().unwrap() + 1;
            self.version.pre =
                Prerelease::new(&format!("alpha.{alpha}")).map_err(|e: semver::Error| Error::Semver(e))?;
        }
        Ok(())
    }

    pub fn rhai_inc_alpha(&mut self) -> RhaiRes<()> {
        self.inc_alpha().map_err(rhai_err)
    }

    pub fn to_string(&mut self) -> String {
        if self.use_v {
            format!("v{}", self.version)
        } else {
            self.version.to_string()
        }
    }
}

impl std::fmt::Display for Semver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.version.fmt(formatter)
    }
}

pub fn semver_rhai_register(engine: &mut Engine) {
    engine
        .register_type_with_name::<Semver>("Semver")
        .register_fn("semver_from", Semver::rhai_parse)
        .register_fn("inc_major", Semver::inc_major)
        .register_fn("inc_minor", Semver::inc_minor)
        .register_fn("inc_patch", Semver::inc_patch)
        .register_fn("inc_beta", Semver::rhai_inc_beta)
        .register_fn("inc_alpha", Semver::rhai_inc_alpha)
        .register_fn("==", |a: Semver, b: Semver| a == b)
        .register_fn("!=", |a: Semver, b: Semver| a != b)
        .register_fn("<", |a: Semver, b: Semver| a < b)
        .register_fn(">", |a: Semver, b: Semver| a > b)
        .register_fn("<=", |a: Semver, b: Semver| a <= b)
        .register_fn(">=", |a: Semver, b: Semver| a >= b)
        .register_fn("to_string", Semver::to_string);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_without_v_prefix() {
        let sv = Semver::parse("1.2.3").unwrap();
        assert_eq!(sv.version.major, 1);
        assert_eq!(sv.version.minor, 2);
        assert_eq!(sv.version.patch, 3);
        assert!(!sv.use_v);
    }

    #[test]
    fn test_parse_with_v_prefix() {
        let sv = Semver::parse("v1.2.3").unwrap();
        assert_eq!(sv.version.major, 1);
        assert_eq!(sv.version.minor, 2);
        assert_eq!(sv.version.patch, 3);
        assert!(sv.use_v);
    }

    #[test]
    fn test_to_string_preserves_v_prefix() {
        // `sv.to_string()` resolves to Display::to_string (omits the "v"),
        // so call the inherent method explicitly.
        let mut sv = Semver::parse("v1.2.3").unwrap();
        assert_eq!(Semver::to_string(&mut sv), "v1.2.3");
    }

    #[test]
    fn test_to_string_without_v_prefix() {
        let mut sv = Semver::parse("1.2.3").unwrap();
        assert_eq!(Semver::to_string(&mut sv), "1.2.3");
    }

    #[test]
    fn test_comparison_lt() {
        let v1 = Semver::parse("1.2.3").unwrap();
        let v2 = Semver::parse("1.2.4").unwrap();
        assert!(v1 < v2);
        assert!(v2 > v1);
    }

    #[test]
    fn test_comparison_eq() {
        let v1 = Semver::parse("1.2.3").unwrap();
        let v2 = Semver::parse("1.2.3").unwrap();
        assert!(v1 == v2);
        assert!(v1 <= v2);
        assert!(v1 >= v2);
    }

    #[test]
    fn test_comparison_major_beats_minor() {
        let v1 = Semver::parse("2.0.0").unwrap();
        let v2 = Semver::parse("1.99.99").unwrap();
        assert!(v1 > v2);
    }

    #[test]
    fn test_comparison_v_prefix_transparent() {
        // v prefix does not affect ordering
        let v1 = Semver::parse("v1.2.3").unwrap();
        let v2 = Semver::parse("2.0.0").unwrap();
        assert!(v1 < v2);
    }

    #[test]
    fn test_inc_major_resets_minor_and_patch() {
        let mut sv = Semver::parse("1.2.3").unwrap();
        sv.inc_major();
        assert_eq!(sv.version.major, 2);
        assert_eq!(sv.version.minor, 0);
        assert_eq!(sv.version.patch, 0);
        assert!(sv.version.pre.is_empty());
    }

    #[test]
    fn test_inc_minor_resets_patch() {
        let mut sv = Semver::parse("1.2.3").unwrap();
        sv.inc_minor();
        assert_eq!(sv.version.minor, 3);
        assert_eq!(sv.version.patch, 0);
        assert!(sv.version.pre.is_empty());
    }

    #[test]
    fn test_inc_patch_stable() {
        let mut sv = Semver::parse("1.2.3").unwrap();
        sv.inc_patch();
        assert_eq!(sv.version.patch, 4);
    }

    #[test]
    fn test_inc_patch_clears_prerelease_without_bumping_patch() {
        let mut sv = Semver::parse("1.2.3-beta.1").unwrap();
        sv.inc_patch();
        // pre is cleared but patch stays at 3
        assert_eq!(sv.version.patch, 3);
        assert!(sv.version.pre.is_empty());
    }

    #[test]
    fn test_inc_beta_from_stable_bumps_patch() {
        let mut sv = Semver::parse("1.2.3").unwrap();
        sv.inc_beta().unwrap();
        assert_eq!(sv.version.patch, 4);
        assert_eq!(sv.version.pre.as_str(), "beta.1");
    }

    #[test]
    fn test_inc_beta_from_existing_beta_increments_counter() {
        let mut sv = Semver::parse("1.2.4-beta.1").unwrap();
        sv.inc_beta().unwrap();
        assert_eq!(sv.version.patch, 4);
        assert_eq!(sv.version.pre.as_str(), "beta.2");
    }

    #[test]
    fn test_inc_alpha_from_stable_bumps_patch() {
        let mut sv = Semver::parse("1.2.3").unwrap();
        sv.inc_alpha().unwrap();
        assert_eq!(sv.version.patch, 4);
        assert_eq!(sv.version.pre.as_str(), "alpha.1");
    }

    #[test]
    fn test_inc_alpha_from_existing_alpha_increments_counter() {
        let mut sv = Semver::parse("1.2.4-alpha.2").unwrap();
        sv.inc_alpha().unwrap();
        assert_eq!(sv.version.pre.as_str(), "alpha.3");
    }

    #[test]
    fn test_parse_invalid_returns_error() {
        assert!(Semver::parse("not-semver").is_err());
        assert!(Semver::parse("1.2").is_err());
        assert!(Semver::parse("").is_err());
    }

    #[test]
    fn test_opt_parse_invalid_returns_none() {
        assert!(Semver::opt_parse("not-semver").is_none());
    }

    #[test]
    fn test_opt_parse_valid_returns_some() {
        assert!(Semver::opt_parse("1.0.0").is_some());
    }

    #[test]
    fn test_prerelease_is_less_than_stable() {
        let pre = Semver::parse("1.2.3-alpha.1").unwrap();
        let stable = Semver::parse("1.2.3").unwrap();
        assert!(pre < stable);
    }
}
