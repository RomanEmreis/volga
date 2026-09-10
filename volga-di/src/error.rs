//! Describes dependency injection errors

use std::fmt::{Display, Formatter};

/// Describes dependency injection error
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum Error {
    /// Indicates that the DI container is missing or not configured
    ContainerMissing,

    /// Indicates that the DI container couldn't resolve a service
    ResolveFailed(&'static str),

    /// Indicates that a requests service has not been registered in the DI container
    NotRegistered(&'static str),

    /// Indicates any other error
    Other(&'static str),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ContainerMissing => write!(f, "Services Error: DI container is missing"),
            Error::ResolveFailed(type_name) => write!(
                f,
                "Services Error: unable to resolve the service: {type_name}"
            ),
            Error::NotRegistered(type_name) => {
                write!(f, "Services Error: service not registered: {type_name}")
            }
            Error::Other(msg) => write!(f, "{msg}"),
        }
    }
}

/// Problems found in a container's dependency graph by
/// [`ContainerBuilder::validate`](crate::ContainerBuilder::validate)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    issues: Vec<Issue>,
}

/// One problem in a container's dependency graph
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    /// Services that depend on one another in a loop, from the first back to itself:
    /// `[A, B, A]` for `A -> B -> A`
    Cycle(Vec<&'static str>),

    /// A service declares a dependency on a type nothing registered
    Missing {
        /// The service declaring the dependency
        service: &'static str,
        /// The type it depends on
        dependency: &'static str,
    },
}

impl ValidationError {
    #[inline]
    pub(crate) fn new(issues: Vec<Issue>) -> Self {
        Self { issues }
    }

    /// Every problem found: cycles first, then missing dependencies, each in a stable order
    #[inline]
    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }
}

impl Display for Issue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Issue::Cycle(path) => write!(f, "dependency cycle: {}", path.join(" -> ")),
            Issue::Missing {
                service,
                dependency,
            } => write!(
                f,
                "`{service}` depends on `{dependency}`, which is not registered"
            ),
        }
    }
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("dependency injection: ")?;

        for (i, issue) in self.issues.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{issue}")?;
        }

        Ok(())
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_displays_a_cycle() {
        assert_eq!(
            Issue::Cycle(vec!["A", "B", "A"]).to_string(),
            "dependency cycle: A -> B -> A"
        );
    }

    #[test]
    fn it_displays_a_missing_dependency() {
        let issue = Issue::Missing {
            service: "A",
            dependency: "B",
        };
        assert_eq!(
            issue.to_string(),
            "`A` depends on `B`, which is not registered"
        );
    }

    #[test]
    fn it_displays_every_issue_on_one_line() {
        let err = ValidationError::new(vec![
            Issue::Cycle(vec!["A", "A"]),
            Issue::Missing {
                service: "B",
                dependency: "C",
            },
        ]);
        assert_eq!(
            err.to_string(),
            "dependency injection: dependency cycle: A -> A; `B` depends on `C`, which is not registered"
        );
    }

    #[test]
    fn it_displays_container_missing() {
        assert_eq!(
            format!("{}", Error::ContainerMissing),
            "Services Error: DI container is missing"
        );
    }

    #[test]
    fn it_displays_resolve_failed() {
        assert_eq!(
            format!("{}", Error::ResolveFailed("Type")),
            "Services Error: unable to resolve the service: Type"
        );
    }

    #[test]
    fn it_displays_not_registered() {
        assert_eq!(
            format!("{}", Error::NotRegistered("Type")),
            "Services Error: service not registered: Type"
        );
    }

    #[test]
    fn it_displays_other() {
        assert_eq!(format!("{}", Error::Other("some error")), "some error");
    }
}
