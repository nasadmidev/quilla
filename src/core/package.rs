use crate::{ Error };
use std::cmp::Ordering;
use std::str::FromStr;
use serde::{Serialize, Deserialize};
use std::env::consts::ARCH;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub epoch: u32,
    pub upstream: String,
    pub release: Option<String>,
}

impl Version {
    pub fn cmp_for_constraint(&self, required: &Version) -> Ordering {
        let base = self
            .epoch
            .cmp(&required.epoch)
            .then_with(|| vercmp(&self.upstream, &required.upstream));

        match &required.release {
            None => base,
            Some(req_rel) => base.then_with(|| match &self.release {
                Some(rel) => vercmp(rel, req_rel),
                None => Ordering::Less,
            }),
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.epoch
            .cmp(&other.epoch)
            .then_with(|| vercmp(&self.upstream, &other.upstream))
            .then_with(|| match (&self.release, &other.release) {
                (Some(a), Some(b)) => vercmp(a, b),
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl FromStr for Version {
    type Err = PackageError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.trim();
        let invalid = || PackageError::InvalidVersion(s.to_string());
        if s.is_empty() {
            return Err(invalid());
        }

        let (epoch, rest) = match s.split_once(':') {
            Some((e, rest)) => (e.parse::<u32>().map_err(|_| invalid())?, rest),
            None => (0, s),
        };

        let (upstream, release) = match rest.rsplit_once('-') {
            Some((up, rel)) => (up, Some(rel)),
            None => (rest, None),
        };

        let upstream_ok = !upstream.is_empty()
            && upstream
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+'));

        if !upstream_ok {
            return Err(invalid());
        }

        if let Some(rel) = release {
            let rel_ok = !rel.is_empty() && rel.chars().all(|c| c.is_ascii_digit() || c == '.');
            if !rel_ok {
                return Err(invalid());
            }
        }

        Ok(Version {
            epoch,
            upstream: upstream.to_string(),
            release: release.map(str::to_string),
        })
    }
}

impl Eq for Version {}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.epoch > 0 {
            write!(f, "{}:", self.epoch)?;
        }
        write!(f, "{}", self.upstream)?;
        if let Some(rel) = &self.release {
            write!(f, "-{rel}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub constraint: Constraint,
}

impl FromStr for Dependency {
    type Err = PackageError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.trim();

        let (name, constraint) = match s.find(|c| matches!(c, '<' | '>' | '=')) {
            None => (s, Constraint::Any),
            Some(pos) => {
                let (name, op_and_version) = s.split_at(pos);

                let constraint = if let Some(v) = op_and_version.strip_prefix(">=") {
                    Constraint::Ge(v.parse()?)
                } else if let Some(v) = op_and_version.strip_prefix("<=") {
                    Constraint::Le(v.parse()?)
                } else if let Some(v) = op_and_version.strip_prefix('>') {
                    Constraint::Gt(v.parse()?)
                } else if let Some(v) = op_and_version.strip_prefix('<') {
                    Constraint::Lt(v.parse()?)
                } else if let Some(v) = op_and_version.strip_prefix('=') {
                    // Pacman also uses `name=soname` for exact shared-library
                    // ABI dependencies, for example
                    // `libwlroots-0.20.so=libwlroots-0.20.so-64`.
                    // These are identities, not package versions.
                    if name.contains(".so") && is_valid_name(v) {
                        Constraint::Soname(v.to_string())
                    } else {
                        Constraint::Eq(v.parse()?)
                    }
                } else {
                    unreachable!()
                };

                (name, constraint)
            }
        };

        if !is_valid_name(name) {
            return Err(PackageError::InvalidName(name.to_string()));
        }

        Ok(Dependency {
            name: name.to_string(),
            constraint,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Dependencies range
pub enum Constraint {
    Any,
    Eq(Version),
    Soname(String),
    Ge(Version),
    Gt(Version),
    Le(Version),
    Lt(Version),
}

impl Constraint {
    pub fn matches(&self, version: &Version) -> bool {
        use Ordering::*;
        match self {
            Constraint::Any => true,
            Constraint::Eq(req) => version.cmp_for_constraint(req) == Equal,
            Constraint::Soname(_) => false,
            Constraint::Ge(req) => version.cmp_for_constraint(req) != Less,
            Constraint::Gt(req) => version.cmp_for_constraint(req) == Greater,
            Constraint::Le(req) => version.cmp_for_constraint(req) != Greater,
            Constraint::Lt(req) => version.cmp_for_constraint(req) == Less,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    #[serde(rename = "x86_64")]
    X86_64,
    Aarch64,
    Any,
}

impl Arch {
    pub fn get_architecture_by_consts() -> Self {
        match ARCH {
            "x86_64" => Self::X86_64,
            "aarch64" => Self::Aarch64,
            _ => Self::Any
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: Version,
    pub arch: Arch,
    pub description: String,
    pub url: Option<String>,
    pub licenses: Vec<String>,
    pub installed_size: u64,
    pub depends: Vec<Dependency>,
    pub conflicts: Vec<Dependency>,
    pub provides: Vec<Dependency>,
    pub replaces: Vec<Dependency>,
}

impl PackageMeta {
    pub fn satisfies(&self, dep: &Dependency) -> bool {
        if self.name == dep.name
            && !matches!(dep.constraint, Constraint::Soname(_))
            && dep.constraint.matches(&self.version)
        {
            return true;
        }

        self.provides.iter().any(|p| {
            p.name == dep.name
                && match &p.constraint {
                    Constraint::Eq(v) => dep.constraint.matches(v),
                    Constraint::Any => dep.constraint == Constraint::Any,
                    Constraint::Soname(provided) => {
                        matches!(&dep.constraint, Constraint::Soname(required) if required == provided)
                    }
                    _ => false,
                }
        })
    }

    /* pub fn parse(pkg: File) -> Result<Self>  {
        let mut package = Self {
            name: String::new(),
            version: Version { epoch: 0, upstream: "".into(), release: None },
            provides: Vec::new(),
            arch: Arch::Any,
            description: String::new(),
            depends: Vec::new(),
            licenses: Vec::new(),
            url: None,
            installed_size: 0,
            conflicts: Vec::new(),
            replaces: Vec::new(),
        };
        let reader = BufReader::new(pkg);
        for (i, l) in reader.lines().enumerate() {
            if let Ok(line) = l {
                if line.is_empty() || line.contains('#') {
                    continue;
                }

                let (key, value) = match line.split_once('=') {
                    Some((k, v)) => (k.trim(), v.trim()),
                    None => {
                        return Err(PackageError::InvalidLine(i, line).into())
                    }
                };

                match key {
                    "pkgname" => {
                        package.name = value.to_string();
                    },
                    "pkgver" => {
                        package.version = value.parse::<Version>()?;
                    },
                    "pkgdesc" => {
                        package.description = value.to_string();
                    },
                    "arch" => {
                        package.arch = match value {
                            "x86_64" => Arch::X86_64,
                            "aarch64" => Arch::Aarch64,
                            _ => Arch::Any,
                        }
                    },
                    "url" => {
                        package.url = Some(value.to_string())
                    },
                    "license" => {
                        package.licenses.push(value.to_string());
                    },
                    "size" => {
                        package.installed_size = match value.parse() {
                            Ok(n) => n,
                            Err(_) => {
                                return Err(PackageError::InvalidLine(i, line).into());
                            }
                        }
                    },
                    "depend" => {
                        package.depends.push(value.parse::<Dependency>()?);
                    },
                    "provide" => {
                        package.provides.push(value.parse::<Dependency>()?);
                    },
                    "replace" => {
                        package.replaces.push(value.parse::<Dependency>()?);
                    },
                    "conflict" => {
                        package.conflicts.push(value.parse::<Dependency>()?);
                    },
                    _ => {
                        return Err(PackageError::InvalidLine(i, line).into());
                    }
                }
            }
        }

        if package.name.is_empty() {
            return Err(PackageError::MissingField("pkgname".into()).into());
        }

        if package.version.upstream.is_empty() {
            return Err(PackageError::MissingField("pkgver".into()).into());
        }

        Ok(package)
    } */
}

#[derive(Debug)]
pub enum PackageError {
    MissingField(String),
    InvalidVersion(String),
    InvalidLine(usize, String),
    InvalidName(String),
    PkgNotFound,
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageError::InvalidLine(line, content) => {
                write!(f, "Invalid line: {content} ({line})")
            }
            PackageError::MissingField(field) => write!(f, "Invalid field: {field}"),
            PackageError::InvalidVersion(version) => write!(f, "Invalid version: {version}"),
            PackageError::InvalidName(name) => write!(f, "Invalid package name: {name}"),
            PackageError::PkgNotFound => write!(f, "Cannot open PKGINFO"),
        }
    }
}

impl From<PackageError> for Error {
    fn from(e: PackageError) -> Self {
        Error::Package(e)
    }
}

/// Integration of Pacman `vercmp`
pub fn vercmp(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }

    let a = a.as_bytes();
    let b = b.as_bytes();
    let (mut i, mut j) = (0, 0);
    loop {
        while i < a.len() && !a[i].is_ascii_alphanumeric() {
            i += 1;
        }

        while j < b.len() && !b[j].is_ascii_alphanumeric() {
            j += 1;
        }

        if i >= a.len() || j >= b.len() {
            break;
        }

        let a_num = a[i].is_ascii_digit();
        let b_num = b[j].is_ascii_digit();

        if a_num != b_num {
            return if a_num {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }

        let seg_a = take_segment(a, &mut i, a_num);
        let seg_b = take_segment(b, &mut j, b_num);

        let ord = if a_num {
            cmp_numeric(seg_a, seg_b)
        } else {
            seg_a.cmp(seg_b)
        };

        if ord != Ordering::Equal {
            return ord;
        }
    }

    let a_done = i >= a.len();
    let b_done = j >= b.len();

    match (a_done, b_done) {
        (true, true) => Ordering::Equal,
        (true, false) => {
            if b[j].is_ascii_alphabetic() {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (false, true) => {
            if a[i].is_ascii_alphabetic() {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (false, false) => unreachable!(),
    }
}

fn take_segment<'a>(s: &'a [u8], pos: &mut usize, numeric: bool) -> &'a [u8] {
    let start = *pos;
    while *pos < s.len() {
        let c = s[*pos];
        let same_kind = if numeric {
            c.is_ascii_digit()
        } else {
            c.is_ascii_alphabetic()
        };
        if !same_kind {
            break;
        }
        *pos += 1;
    }
    &s[start..*pos]
}

fn cmp_numeric(a: &[u8], b: &[u8]) -> Ordering {
    let a = strip_zeros(a);
    let b = strip_zeros(b);
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn strip_zeros(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|&&c| c == b'0').count();
    &s[n..]
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['-', '.'])
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '-')
        })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::cmp::Ordering::*;

    pub fn v(s: &str) -> Version {
        s.parse().unwrap()
    }
    pub fn d(s: &str) -> Dependency {
        s.parse().unwrap()
    }

    #[test]
    fn vercmp_basic() {
        let cases = [
            ("1.0", "1.0", Equal),
            ("1.10", "1.9", Greater),
            ("1.0", "1.00", Equal),
            ("1.0.1", "1.0", Greater),
            ("1.0rc1", "1.0", Less),
            ("1.0a", "1.0.1", Less),
            ("alpha", "beta", Less),
            ("20261109", "20251231", Greater),
            ("1.0rc10", "1.0rc9", Greater),
            ("1.0beta2", "1.0beta10", Less),
        ];
        for (a, b, expected) in cases {
            assert_eq!(vercmp(a, b), expected, "Normal: {a} vs {b}");
            assert_eq!(vercmp(b, a), expected.reverse(), "Reversed: {b} vs {a}");
        }
    }

    #[test]
    fn parse_version_ok() {
        for s in ["1:2.4.0-3", "2.38", "1.0rc1-2", "1.0-1.1"] {
            assert_eq!(v(s).to_string(), s);
        }
    }

    #[test]
    fn parse_version_err() {
        for s in [
            "", ":1.0", "x:1.0", "1.0-", "-1", "1.0-1-2", "1:2:3", "1.0 beta", "1.0-a",
        ] {
            assert!(s.parse::<Version>().is_err(), "should fail: {s:?}");
        }
    }

    #[test]
    fn parse_dependency_ok() {
        let cases = [
            ("zlib", "zlib", Constraint::Any),
            (
                "lib32-glibc>=2.38",
                "lib32-glibc",
                Constraint::Ge(v("2.38")),
            ),
            ("foo=1:2.0-3", "foo", Constraint::Eq(v("1:2.0-3"))),
            ("glibc>=2.38", "glibc", Constraint::Ge(v("2.38"))),
        ];
        for (dependency, expect_name, expect_constraint) in cases {
            let d: Dependency = d(dependency);
            assert_eq!(d.name, expect_name);
            assert_eq!(d.constraint, expect_constraint);
        }
    }

    #[test]
    fn parse_dependency_err() {
        for s in [">=1.0", "foo>=", "foo=>1"] {
            assert!(s.parse::<Dependency>().is_err());
        }
    }

    #[test]
    fn constraint_matches() {
        let cases = [
            ("glibc>=2.38", "2.38-1", true),
            ("glibc=2.38", "2.38-1", true),
            ("glibc<=2.38", "2.38-2", true),
            ("glibc=2.38-1", "2.38-2", false),
            ("glibc>2.38", "2.38-5", false),
            ("glibc<2.39", "2.38-5", true),
            ("glibc>=2.0", "1:1.0-1", true),
            ("glibc<2.0", "1:1.0-1", false),
            ("glibc>=2.40", "2.38-1", false),
            ("glibc", "0.1-1", true),
        ];

        for (dep, ver, expected) in cases {
            assert_eq!(d(dep).constraint.matches(&v(ver)), expected, "{dep}: {ver}");
        }
    }

    #[test]
    fn satifies_provides() {
        let bash = PackageMeta {
            name: "bash".into(),
            version: v("5.2.37-1"),
            provides: vec![d("sh=5.2")],
            arch: Arch::X86_64,
            description: "GNU Shell".into(),
            depends: vec![],
            licenses: vec![],
            url: None,
            installed_size: 3600,
            conflicts: vec![],
            replaces: vec![],
        };

        assert!(bash.satisfies(&d("bash>=5.0")));
        assert!(bash.satisfies(&d("sh")));
        assert!(bash.satisfies(&d("sh>=5")));
        assert!(!bash.satisfies(&d("sh>=6")));
        assert!(!bash.satisfies(&d("zsh")));
    }
}
