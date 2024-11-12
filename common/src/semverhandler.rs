use crate::{rhai_err, Error, Result, RhaiRes};
use semver::{Prerelease, Version};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Semver {
    pub version: Version,
    pub use_v: bool,
}
// support des rc (release candidate)
impl Semver {
    pub fn parse(str: &str) -> Result<Self> {
        let use_v = str.starts_with("v");
        let version = if use_v {
            let mut chars = str.chars();
            chars.next();
            Version::parse(chars.as_str()).map_err(|e| Error::Semver(e))?
        } else {
            Version::parse(str).map_err(|e| Error::Semver(e))?
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
        Self::parse(str).map_err(|e| rhai_err(e))
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
        self.inc_beta().map_err(|e| rhai_err(e))
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
        self.inc_alpha().map_err(|e| rhai_err(e))
    }

    pub fn to_string(&mut self) -> String {
        if self.use_v {
            format!("v{}", self.version.to_string())
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
