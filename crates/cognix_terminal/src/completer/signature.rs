//! A small registry of command signatures, used to describe commands and suggest their flags.

use std::collections::BTreeMap;

/// A flag a command accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagSignature {
    pub name: String,
    pub description: Option<String>,
    /// Whether the flag takes a value, e.g. `--output <path>`.
    pub takes_value: bool,
}

impl FlagSignature {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            takes_value: false,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_value(mut self) -> Self {
        self.takes_value = true;
        self
    }
}

/// What a command is and what it accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSignature {
    pub name: String,
    pub description: Option<String>,
    pub flags: Vec<FlagSignature>,
    /// Example invocations, used as documentation.
    pub examples: Vec<String>,
}

impl CommandSignature {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            flags: Vec::new(),
            examples: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_flags(mut self, flags: impl IntoIterator<Item = FlagSignature>) -> Self {
        self.flags = flags.into_iter().collect();
        self
    }

    pub fn with_example(mut self, example: impl Into<String>) -> Self {
        self.examples.push(example.into());
        self
    }
}

/// Errors from building or querying the registry.
#[derive(Debug, thiserror::Error)]
pub enum SignatureRegistryError {
    #[error("a signature for `{0}` is already registered")]
    Duplicate(String),
}

/// Known commands and their flags.
///
/// Intentionally small: it covers the commands a terminal user reaches for constantly rather
/// than trying to mirror a full completion database that cannot be shipped here.
#[derive(Clone, Debug, Default)]
pub struct SignatureRegistry {
    signatures: BTreeMap<String, CommandSignature>,
}

impl SignatureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `signature`, rejecting a duplicate name.
    pub fn register(&mut self, signature: CommandSignature) -> Result<(), SignatureRegistryError> {
        let name = signature.name.clone();
        if self.signatures.contains_key(&name) {
            return Err(SignatureRegistryError::Duplicate(name));
        }
        self.signatures.insert(name, signature);
        Ok(())
    }

    /// Registers without the duplicate check.
    pub fn insert(&mut self, signature: CommandSignature) {
        self.signatures.insert(signature.name.clone(), signature);
    }

    pub fn get(&self, name: &str) -> Option<&CommandSignature> {
        self.signatures.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.signatures.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.signatures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signatures.is_empty()
    }

    /// The default registry, covering common developer commands.
    pub fn builtin() -> Self {
        let mut registry = Self::new();

        /// (command, description, [(flag, description)])
        type BuiltinEntry = (&'static str, &'static str, &'static [(&'static str, &'static str)]);

        let simple: &[BuiltinEntry] = &[
            (
                "git",
                "Distributed version control",
                &[
                    ("--status", "Show working tree status"),
                    ("--log", "Show commit logs"),
                    ("--diff", "Show changes"),
                    ("--push", "Update remote refs"),
                    ("--pull", "Fetch and integrate"),
                    ("--commit", "Record changes"),
                ],
            ),
            (
                "cargo",
                "Rust's package manager",
                &[
                    ("--build", "Compile the project"),
                    ("--check", "Type check without codegen"),
                    ("--test", "Run tests"),
                    ("--run", "Run a binary target"),
                    ("--clippy", "Run lints"),
                    ("--fmt", "Format the project"),
                    ("--release", "Build with optimizations"),
                ],
            ),
            (
                "ls",
                "List directory contents",
                &[
                    ("--all", "Include entries starting with ."),
                    ("--long", "Use a long listing format"),
                    ("--human-readable", "Print sizes in human readable form"),
                ],
            ),
            (
                "grep",
                "Print lines matching a pattern",
                &[
                    ("--recursive", "Recurse into subdirectories"),
                    ("--line-number", "Prefix each line with its number"),
                    ("--ignore-case", "Ignore case distinctions"),
                    ("--invert-match", "Select non-matching lines"),
                ],
            ),
            (
                "docker",
                "Build and run containerized apps",
                &[
                    ("--build", "Build an image"),
                    ("--run", "Create and run a container"),
                    ("--ps", "List containers"),
                    ("--logs", "Fetch container logs"),
                ],
            ),
            (
                "kubectl",
                "Control a Kubernetes cluster",
                &[
                    ("--get", "Display one or many resources"),
                    ("--apply", "Apply a configuration"),
                    ("--describe", "Show details of a resource"),
                    ("--logs", "Print logs for a pod"),
                ],
            ),
        ];

        for (name, description, flags) in simple {
            let mut signature = CommandSignature::new(*name).with_description(*description);
            for (flag, doc) in flags.iter() {
                signature.flags.push(FlagSignature::new(*flag).with_description(*doc));
            }
            registry.insert(signature);
        }

        // Flags that take values, so completion can tell them apart.
        registry.insert(
            CommandSignature::new("tar")
                .with_description("Archive files")
                .with_flags([
                    FlagSignature::new("--extract").with_description("Extract from archive"),
                    FlagSignature::new("--create")
                        .with_description("Create a new archive")
                        .with_value(),
                    FlagSignature::new("--file")
                        .with_description("Archive file name")
                        .with_value(),
                ])
                .with_example("tar --create --file backup.tar src"),
        );

        registry
    }
}

#[cfg(test)]
#[path = "signature_tests.rs"]
mod tests;