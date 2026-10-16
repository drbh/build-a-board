//! Typed nets.

use std::collections::BTreeSet;
use std::fmt::{self, Display};
use std::sync::{Mutex, PoisonError};

/// A net. Declare a board's nets once with [`nets!`](crate::nets) and use
/// the constants everywhere, so a misspelt net is a compile error; nets
/// whose names are computed come from [`Net::named`] or a [`Scope`].
///
/// [`NC`] is the net of a pin that is deliberately left unwired.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Net(Option<&'static str>);

/// No connection: a part pin wired to `NC` gets no net.
pub const NC: Net = Net(None);

/// Names created at run time live for the program; each is leaked once.
static NAMED: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());

impl Net {
    pub const fn new(name: &'static str) -> Self {
        Self(Some(name))
    }

    /// A net named at run time: `Net::named(format!("D{i}"))`. Two nets with
    /// the same name are the same net.
    pub fn named(name: impl Display) -> Self {
        let name = name.to_string();
        let mut named = NAMED.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = named.get(name.as_str()) {
            return Self(Some(existing));
        }
        let name: &'static str = Box::leak(name.into_boxed_str());
        named.insert(name);
        Self(Some(name))
    }

    /// The net's name; `NC` is named `"NC"`.
    pub const fn name(self) -> &'static str {
        match self.0 {
            Some(name) => name,
            None => "NC",
        }
    }

    /// Whether this is [`NC`], the unwired pin.
    pub const fn is_nc(self) -> bool {
        self.0.is_none()
    }
}

impl From<&Net> for Net {
    fn from(net: &Net) -> Self {
        *net
    }
}

impl Display for Net {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A block's private nets: `Scope::new("buck").net("SW")` is the net
/// `buck.SW`, so a block instanced twice has two sets of internal nets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope(String);

impl Scope {
    pub fn new(name: impl Display) -> Self {
        Self(name.to_string())
    }

    /// The net `name` inside this scope.
    pub fn net(&self, name: impl Display) -> Net {
        Net::named(format!("{}.{name}", self.0))
    }
}

/// Declare nets as constants:
///
/// ```
/// bb_sdk::nets! {
///     /// Battery after the fuse.
///     VBAT = "VBAT",
///     GND = "GND",
///     V6 = "6V",
/// }
/// assert_eq!(V6.name(), "6V");
/// ```
#[macro_export]
macro_rules! nets {
    ($( $(#[$meta:meta])* $name:ident = $net:literal ),* $(,)?) => {
        $( $(#[$meta])* pub const $name: $crate::Net = $crate::Net::new($net); )*
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_nets_compare_by_name() {
        assert_eq!(Net::named(format!("D{}", 1)), Net::new("D1"));
        assert_eq!(Scope::new("buck").net("SW").name(), "buck.SW");
        assert_ne!(Scope::new("a").net("SW"), Scope::new("b").net("SW"));
        assert!(NC.is_nc() && !Net::new("GND").is_nc());
    }
}
