//! Parts as types: named pins, typed values, and handles on placed parts.
//!
//! A [`Component`] is a kind of part wired to nets. Placing it returns a
//! [`Placed`] handle whose methods name its pads, so routes say `u1.sw()`
//! rather than `pin("U1", 8)`, and a part that does not exist cannot be named.

use std::fmt::{self, Display};
use std::ops::Deref;
use std::rc::Rc;

use crate::units::{Farads, Ohms, Volts};
use crate::{At, Axis, Generated, IntoItems, Item, Model, Net, Part, Pin, PinLegends, Point, part};

/// A placed part's reference designator. Its pads are anchors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ref(Rc<str>);

impl Ref {
    pub fn new(reference: &str) -> Self {
        Self(reference.into())
    }

    pub fn name(&self) -> &str {
        &self.0
    }

    /// The centre of pad `pad`, resolved by `bb` once every part is placed.
    pub fn pad(&self, pad: impl Display) -> At {
        At::Pin(Pin { reference: self.0.to_string(), pad: pad.to_string() })
    }
}

impl Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A pin that spans several pads (a connector's four VBUS pads). As an
/// anchor it is the first pad; `.pad(n)` names another.
#[derive(Clone, Debug)]
pub struct Pads {
    reference: Ref,
    pads: &'static [&'static str],
}

impl Pads {
    #[doc(hidden)]
    pub fn new(reference: Ref, pads: &'static [&'static str]) -> Self {
        Self { reference, pads }
    }

    /// One of this pin's pads. Panics if `pad` is not among them.
    #[track_caller]
    pub fn pad(&self, pad: impl Display) -> At {
        let pad = pad.to_string();
        assert!(
            self.pads.contains(&pad.as_str()),
            "{} has no pad {pad} on this pin; it has {:?}",
            self.reference,
            self.pads
        );
        self.reference.pad(pad)
    }
}

impl From<Pads> for At {
    fn from(pads: Pads) -> Self {
        pads.reference.pad(pads.pads[0])
    }
}

/// A footprint, from a library or generated.
#[derive(Clone, Debug, PartialEq)]
pub enum Footprint {
    Library(Part),
    Generated(Generated),
}

impl From<Part> for Footprint {
    fn from(part: Part) -> Self {
        Self::Library(part)
    }
}

impl From<Generated> for Footprint {
    fn from(generated: Generated) -> Self {
        Self::Generated(generated)
    }
}

/// A footprint at a position: what a [`Placed`] part turns into.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub footprint: Footprint,
    pub at: Point,
    /// Degrees counter-clockwise on screen.
    pub rotate: f64,
    /// Hide the designator even when its prefix is not on the board's list.
    pub hide_reference: bool,
}

/// The pad handles of a placed part: made from its reference, and from the
/// part itself when the handle needs to know how it was wired.
pub trait Handle<C>: Sized {
    fn new(part: &C, reference: Ref) -> Self;
}

/// A kind of part, wired to nets.
pub trait Component: Sized {
    /// Named anchors on the placed part's pads.
    type Pins: Handle<Self>;

    /// The footprint, wired to this part's nets.
    fn footprint(&self, reference: &str) -> Footprint;

    /// Place the part as `reference` with its origin at `at`.
    fn place(self, reference: &str, at: impl Into<Point>) -> Placed<Self> {
        let reference = Ref::new(reference);
        Placed {
            footprint: self.footprint(reference.name()),
            pins: Self::Pins::new(&self, reference.clone()),
            reference,
            at: at.into(),
            rotate: 0.0,
            hide_reference: false,
            part: self,
        }
    }
}

/// A part on the board. Dereferences to its pins: `u1.sw()`.
pub struct Placed<C: Component> {
    /// What was placed: its nets and values.
    pub part: C,
    reference: Ref,
    footprint: Footprint,
    at: Point,
    rotate: f64,
    hide_reference: bool,
    pins: C::Pins,
}

impl<C: Component> Placed<C> {
    /// Rotate counter-clockwise about the part origin (KiCad's convention).
    pub fn rotate(mut self, degrees: impl Into<f64>) -> Self {
        self.rotate = degrees.into();
        self
    }

    /// Print no silkscreen designator for this part.
    pub fn hide_reference(mut self) -> Self {
        self.hide_reference = true;
        self
    }

    pub fn reference(&self) -> &Ref {
        &self.reference
    }

    /// Any pad by number, for pads without a name.
    pub fn pad(&self, pad: impl Display) -> At {
        self.reference.pad(pad)
    }

    /// One legend per named pad in a row at `y` (in the enclosing frame),
    /// each above its pad, rotated 90°.
    pub fn legends_row<A: Display, B: Display>(
        &self,
        y: impl Into<f64>,
        legends: impl IntoIterator<Item = (A, B)>,
    ) -> PinLegends {
        PinLegends {
            reference: self.reference.to_string(),
            legends: crate::pairs(legends),
            axis: Axis::Row(y.into()),
            size: 0.6,
            rotate: 90.0,
            layer: crate::Layer::FSilk,
        }
    }

    /// One legend per named pad in a column at `x`, each beside its pad.
    pub fn legends_column<A: Display, B: Display>(
        &self,
        x: impl Into<f64>,
        legends: impl IntoIterator<Item = (A, B)>,
    ) -> PinLegends {
        PinLegends { axis: Axis::Column(x.into()), rotate: 0.0, ..self.legends_row(0, legends) }
    }

    fn placement(&self) -> Placement {
        Placement {
            footprint: self.footprint.clone(),
            at: self.at,
            rotate: self.rotate,
            hide_reference: self.hide_reference,
        }
    }
}

impl<C: Component + Clone> Clone for Placed<C>
where
    C::Pins: Clone,
{
    fn clone(&self) -> Self {
        Self {
            part: self.part.clone(),
            reference: self.reference.clone(),
            footprint: self.footprint.clone(),
            at: self.at,
            rotate: self.rotate,
            hide_reference: self.hide_reference,
            pins: self.pins.clone(),
        }
    }
}

impl<C: Component> Deref for Placed<C> {
    type Target = C::Pins;

    fn deref(&self) -> &C::Pins {
        &self.pins
    }
}

impl<C: Component> IntoItems for Placed<C> {
    fn into_items(self, out: &mut Vec<Item>) {
        out.push(Item::Placement(Placement {
            footprint: self.footprint,
            at: self.at,
            rotate: self.rotate,
            hide_reference: self.hide_reference,
        }));
    }
}

impl<C: Component> IntoItems for &Placed<C> {
    fn into_items(self, out: &mut Vec<Item>) {
        out.push(Item::Placement(self.placement()));
    }
}

impl<C: Component, const N: usize> IntoItems for &[Placed<C>; N] {
    fn into_items(self, out: &mut Vec<Item>) {
        self.iter().for_each(|placed| placed.into_items(out));
    }
}

/// Declare a part: a struct with one [`Net`] per named pin, and a handle
/// type with one anchor method per pin.
///
/// ```
/// use bb_sdk::*;
///
/// part! {
///     /// A USB 2.0 receptacle.
///     pub struct UsbC => UsbCPins {
///         /// Four pads, one net.
///         vbus: ["A4", "A9", "B4", "B9"],
///         dp: ["A6", "B6"],
///         cc1: "A5",
///         shield: "SH",
///     }
///     footprint = "Connector_USB:USB_C_Receptacle_HRO_TYPE-C-31-M-12", value = "USB-C"
/// }
///
/// let usb = UsbC { vbus: Net::new("VBUS"), dp: Net::new("D+"), cc1: Net::new("CC1"), shield: NC };
/// assert_eq!(usb.pads().len(), 7); // the shield is left unwired
/// let j1 = usb.place("J1", (0, 0));
/// let _anchors = [j1.cc1(), j1.vbus().into(), j1.vbus().pad("B9")];
/// ```
///
/// Writing the struct literal names every pin, so none can be left unwired
/// by accident; a pin wired to [`NC`](crate::NC) is unwired on purpose. A
/// pin on several pads is written `vbus: ["A4", "A9"]`; its handle method
/// returns [`Pads`]. With `footprint = ..., value = ...` the part is a
/// [`Component`]; leave the clause off and implement [`Component::footprint`]
/// when the footprint or value depends on the fields.
#[macro_export]
macro_rules! part {
    // A part with no pins: a hole, a fiducial.
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident => $pins:ident {}
        $($rest:tt)*
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis struct $name;

        impl $name {
            /// Pad number → net, for the footprint.
            pub fn pads(&self) -> ::std::vec::Vec<(::std::string::String, $crate::Net)> {
                ::std::vec::Vec::new()
            }
        }

        $crate::__part_handle! { $vis $name => $pins {} $($rest)* }
    };
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident => $pins:ident {
            $( $(#[$pin_meta:meta])* $pin:ident: $pads:tt ),+ $(,)?
        }
        $($rest:tt)*
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis struct $name {
            $( $(#[$pin_meta])* pub $pin: $crate::Net, )+
        }

        impl $name {
            /// Pad number → net, for the footprint; `NC` pins are left out.
            pub fn pads(&self) -> ::std::vec::Vec<(::std::string::String, $crate::Net)> {
                let mut pads = ::std::vec::Vec::new();
                $(
                    for pad in $crate::__pads!($pads) {
                        pads.push((::std::string::ToString::to_string(pad), self.$pin));
                    }
                )+
                pads.retain(|(_, net)| !net.is_nc());
                pads
            }
        }

        $crate::__part_handle! { $vis $name => $pins { $( $(#[$pin_meta])* $pin: $pads ),+ } $($rest)* }
    };
}

/// The handle type and optional `Component` impl behind [`part!`].
#[doc(hidden)]
#[macro_export]
macro_rules! __part_handle {
    (
        $vis:vis $name:ident => $pins:ident {
            $( $(#[$pin_meta:meta])* $pin:ident: $pads:tt ),*
        }
        $( footprint = $footprint:expr, value = $value:expr $(,)? )?
    ) => {
        #[doc = concat!("The pads of a placed [`", stringify!($name), "`].")]
        #[derive(Clone, Debug)]
        $vis struct $pins(pub $crate::Ref);

        impl<C> $crate::Handle<C> for $pins {
            fn new(_: &C, reference: $crate::Ref) -> Self {
                Self(reference)
            }
        }

        impl $pins {
            $( $(#[$pin_meta])* pub fn $pin(&self) -> $crate::__pin_type!($pads) { $crate::__pin!(self.0, $pads) } )*
        }

        $(
            impl $crate::Component for $name {
                type Pins = $pins;

                fn footprint(&self, reference: &str) -> $crate::Footprint {
                    $crate::part(reference, $value, $footprint).pins(self.pads()).into()
                }
            }
        )?
    };
}

/// A pin's pads as a string slice: `1` → `&["1"]`, `["A4", "B4"]` → both.
#[doc(hidden)]
#[macro_export]
macro_rules! __pads {
    ([$($pad:literal),+ $(,)?]) => { &[$(concat!($pad)),+] };
    ($pad:literal) => { &[concat!($pad)] };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __pin_type {
    ([$($pad:literal),+ $(,)?]) => {
        $crate::Pads
    };
    ($pad:literal) => {
        $crate::At
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __pin {
    ($reference:expr, [$($pad:literal),+ $(,)?]) => { $crate::Pads::new($reference.clone(), &[$(concat!($pad)),+]) };
    ($reference:expr, $pad:literal) => { $reference.pad(concat!($pad)) };
}

part! {
    /// The two terminals of a passive part.
    pub struct TwoPin => TwoPinPins {
        p1: 1,
        p2: 2,
    }
}

/// A chip resistor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resistor {
    pub value: Ohms,
    /// Tolerance in percent, printed after the value: `10k 1%`.
    pub tolerance: Option<f64>,
    pub footprint: &'static str,
    pub nets: TwoPin,
}

/// A resistor from `p1` to `p2`, in 0603 unless refined.
pub fn resistor(value: Ohms, p1: Net, p2: Net) -> Resistor {
    Resistor { value, tolerance: None, footprint: crate::R0603, nets: TwoPin { p1, p2 } }
}

impl Resistor {
    pub fn tolerance(mut self, percent: impl Into<f64>) -> Self {
        self.tolerance = Some(percent.into());
        self
    }

    pub fn footprint(mut self, footprint: &'static str) -> Self {
        self.footprint = footprint;
        self
    }
}

impl Component for Resistor {
    type Pins = TwoPinPins;

    fn footprint(&self, reference: &str) -> Footprint {
        let value = match self.tolerance {
            Some(tolerance) => format!("{} {tolerance}%", self.value),
            None => self.value.to_string(),
        };
        part(reference, value, self.footprint).pins(self.nets.pads()).into()
    }
}

/// A ceramic capacitor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capacitor {
    pub value: Farads,
    /// Voltage rating, printed after the value: `100n 50V`.
    pub rating: Option<Volts>,
    pub footprint: &'static str,
    pub nets: TwoPin,
}

/// A capacitor from `p1` to `p2`, in 0603 unless refined.
pub fn capacitor(value: Farads, p1: Net, p2: Net) -> Capacitor {
    Capacitor { value, rating: None, footprint: crate::C0603, nets: TwoPin { p1, p2 } }
}

impl Capacitor {
    pub fn rated(mut self, voltage: Volts) -> Self {
        self.rating = Some(voltage);
        self
    }

    pub fn footprint(mut self, footprint: &'static str) -> Self {
        self.footprint = footprint;
        self
    }
}

impl Component for Capacitor {
    type Pins = TwoPinPins;

    fn footprint(&self, reference: &str) -> Footprint {
        let value = match self.rating {
            Some(rating) => format!("{} {rating}", self.value),
            None => self.value.to_string(),
        };
        part(reference, value, self.footprint).pins(self.nets.pads()).into()
    }
}

/// A part whose pads are known by number: connectors, headers, sockets.
/// Its handle finds pads by net, `header.on(GND)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Connector {
    template: Part,
}

/// A library connector; wire it with [`Connector::numbered`] or [`Connector::pins`].
pub fn connector(value: impl Into<String>, footprint: &str) -> Connector {
    Connector { template: part("", value, footprint) }
}

impl Connector {
    /// Pads 1..n carry `nets`, in order.
    pub fn numbered(self, nets: impl IntoIterator<Item = impl Into<Net>>) -> Self {
        self.pins(nets.into_iter().enumerate().map(|(i, net)| (i + 1, net.into())))
    }

    /// Pad number → net, for connectors with unwired pads.
    pub fn pins<A: Display>(mut self, pins: impl IntoIterator<Item = (A, Net)>) -> Self {
        self.template = self.template.pins(pins);
        self
    }

    /// Replace the 3D models (an empty list removes them).
    pub fn models(mut self, models: impl IntoIterator<Item = Model>) -> Self {
        self.template = self.template.models(models);
        self
    }
}

/// The pads of a placed [`Connector`].
#[derive(Clone, Debug)]
pub struct ConnectorPins {
    reference: Ref,
    pins: Rc<[(String, String)]>,
}

impl ConnectorPins {
    /// The one pad on `net`. Panics if there is none, or several.
    #[track_caller]
    pub fn on(&self, net: Net) -> At {
        let mut pads = self.pins.iter().filter(|(_, n)| n == net.name());
        match (pads.next(), pads.next()) {
            (Some((pad, _)), None) => self.reference.pad(pad),
            (None, _) => panic!("{} has no pad on {net}", self.reference),
            (Some(_), Some(_)) => panic!("{} has several pads on {net}; name one with .pad(n)", self.reference),
        }
    }
}

impl Handle<Connector> for ConnectorPins {
    fn new(part: &Connector, reference: Ref) -> Self {
        Self { reference, pins: part.template.pins.clone().into() }
    }
}

impl Component for Connector {
    type Pins = ConnectorPins;

    fn footprint(&self, reference: &str) -> Footprint {
        Part { reference: reference.to_owned(), ..self.template.clone() }.into()
    }
}
