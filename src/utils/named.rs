//! Values the user picks by name.
//!
//! The app spells the same value in four places: the config file (through serde), a `:command`
//! argument, the `Tab` completion list, and the toast that reports a change. Each of those used
//! to carry its own copy — a `match` to parse, another to name, a `vec!` for the completion, and
//! a hand-written chain for the bare command's cycle — so adding a value meant editing all four
//! and forgetting one was silent. An enum that implements this trait owns its table once:
//! [`Named::ALL`] is the list, [`Named::name`] is the spelling, and parsing, cycling, completion
//! lists and the error message that lists the options all read those.

/// A value the user picks by name.
pub trait Named: Copy + PartialEq + Sized + 'static {
    /// Every value, in the order the command line offers them.
    const ALL: &'static [Self];

    /// How it is written in the config and typed on the command line.
    fn name(self) -> &'static str;

    /// Other spellings the config accepts: the squashed forms command names favour, and the
    /// serde aliases, so that the two ways in cannot drift apart.
    const ALIASES: &'static [(&'static str, Self)] = &[];

    /// One line for the completion list and the toast; the name, when an enum has nothing more
    /// to say about a value.
    fn describe(self) -> &'static str {
        self.name()
    }

    /// Parse a name, ignoring case and surrounding space.
    fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|value| value.name().eq_ignore_ascii_case(name))
            .or_else(|| {
                Self::ALIASES
                    .iter()
                    .find(|(alias, _)| alias.eq_ignore_ascii_case(name))
                    .map(|(_, value)| *value)
            })
    }

    /// The next value in [`Self::ALL`], wrapping around: what a bare `:command` cycles through.
    fn next(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|value| *value == self)
            .unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// Every name, for the `Tab` completion of a command's argument.
    fn names() -> Vec<&'static str> {
        Self::ALL.iter().map(|value| value.name()).collect()
    }
}

/// Test-only: the contract every [`Named`] enum has with the config file.
///
/// One assertion per enum, instead of the four near-identical test bodies this replaced: a name
/// round trips through serde as the same value, the name parses back, the aliases are spellings
/// of the value they name, no two values share a name, and the bare command's cycle walks `ALL`
/// in `ALL`'s order.
#[cfg(test)]
pub(crate) fn assert_named_contract<T>()
where
    T: Named + std::fmt::Debug + serde::Serialize + serde::de::DeserializeOwned,
{
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct Holder<T> {
        value: T,
    }

    let mut names: Vec<&str> = Vec::new();
    for value in T::ALL {
        let text = toml_edit::ser::to_string(&Holder { value: *value }).expect("serialize");
        let name = value.name();
        assert!(
            text.contains(&format!("value = \"{name}\"")),
            "{value:?} is written as {text}"
        );
        let back: Holder<T> = toml_edit::de::from_str(&text).expect("deserialize");
        assert_eq!(back.value, *value, "{text}");
        assert_eq!(T::parse(name), Some(*value));
        assert_eq!(T::parse(&name.to_uppercase()), Some(*value));
        names.push(name);
    }

    let count = names.len();
    assert!(count > 0, "an enum with no values cannot be picked");
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), count, "two values share a name");

    for (alias, value) in T::ALIASES {
        assert_eq!(T::parse(alias), Some(*value), "alias {alias}");
    }

    let mut cycled = vec![T::ALL[0]];
    let mut walking = T::ALL[0];
    for _ in 1..T::ALL.len() {
        walking = walking.next();
        cycled.push(walking);
    }
    assert_eq!(cycled, T::ALL.to_vec());
    assert_eq!(walking.next(), T::ALL[0]);
}
