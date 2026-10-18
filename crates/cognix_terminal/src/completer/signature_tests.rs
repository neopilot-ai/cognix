use super::*;

#[test]
fn builtin_registry_has_the_expected_commands() {
    let registry = SignatureRegistry::builtin();
    assert!(registry.get("git").is_some());
    assert!(registry.get("cargo").is_some());
    assert!(registry.get("grep").is_some());
    assert!(registry.get("docker").is_some());
    assert!(registry.get("kubectl").is_some());
    assert!(registry.get("not-a-command").is_none());
}

#[test]
fn builtin_signatures_carry_descriptions() {
    let registry = SignatureRegistry::builtin();
    let git = registry.get("git").unwrap();
    assert_eq!(git.description.as_deref(), Some("Distributed version control"));
}

#[test]
fn builtin_signatures_carry_flags() {
    let registry = SignatureRegistry::builtin();
    let cargo = registry.get("cargo").unwrap();
    let names: Vec<&str> = cargo.flags.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"--build"));
    assert!(names.contains(&"--test"));
}

#[test]
fn value_taking_flags_are_marked() {
    let registry = SignatureRegistry::builtin();
    let tar = registry.get("tar").unwrap();
    let file = tar.flags.iter().find(|f| f.name == "--file").unwrap();
    assert!(file.takes_value);
    assert_eq!(file.description.as_deref(), Some("Archive file name"));

    let extract = tar.flags.iter().find(|f| f.name == "--extract").unwrap();
    assert!(!extract.takes_value);
}

#[test]
fn registering_a_duplicate_is_rejected() {
    let mut registry = SignatureRegistry::new();
    registry
        .register(CommandSignature::new("git"))
        .unwrap();
    let err = registry.register(CommandSignature::new("git")).unwrap_err();
    assert!(matches!(err, SignatureRegistryError::Duplicate(name) if name == "git"));
}

#[test]
fn insert_overwrites_rather_than_failing() {
    let mut registry = SignatureRegistry::new();
    registry.insert(CommandSignature::new("x").with_description("first"));
    registry.insert(CommandSignature::new("x").with_description("second"));
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.get("x").unwrap().description.as_deref(), Some("second"));
}

#[test]
fn empty_registry_is_empty() {
    let registry = SignatureRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.names().count(), 0);
}

#[test]
fn signature_builders_compose() {
    let sig = CommandSignature::new("demo")
        .with_description("A demo")
        .with_flags([FlagSignature::new("-v").with_description("verbose").with_value()])
        .with_example("demo -v file");

    assert_eq!(sig.flags.len(), 1);
    assert_eq!(sig.examples, vec!["demo -v file"]);
    assert!(sig.flags[0].takes_value);
}