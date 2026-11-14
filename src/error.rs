use std::fmt;

#[derive(Debug)]
pub enum Failure {
    Preflight(anyhow::Error),
    Git(anyhow::Error),
    Severe(anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Failure>;

impl Failure {
    pub fn code(&self) -> i32 {
        match self {
            Failure::Preflight(_) => 1,
            Failure::Git(_) | Failure::Severe(_) => 2,
        }
    }
    pub fn inner(&self) -> &anyhow::Error {
        match self {
            Failure::Preflight(e) | Failure::Git(e) | Failure::Severe(e) => e,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_codes_match_contract() {
        assert_eq!(Failure::Preflight(anyhow::anyhow!("x")).code(), 1);
        assert_eq!(Failure::Git(anyhow::anyhow!("x")).code(), 2);
        assert_eq!(Failure::Severe(anyhow::anyhow!("x")).code(), 2);
    }
}
