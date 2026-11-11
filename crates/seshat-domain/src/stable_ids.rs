//! Enums whose variants have stable text ids (settings files, window
//! actions, the command line).

/// Declares a field-less enum together with the stable text id of each
/// variant, and generates:
///
/// - `ALL`: every variant, in declaration order, so it can never miss one;
/// - `id(self)`: the variant's id;
/// - `from_id(&str)`: the variant with that id, if any.
///
/// Attributes and doc comments on the enum and its variants are kept. The
/// enum must be `Copy`.
///
/// ```
/// seshat_domain::stable_ids! {
///     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
///     pub enum Shape {
///         Circle => "circle",
///         /// Four equal sides.
///         Square => "square",
///     }
/// }
/// assert_eq!(Shape::ALL, [Shape::Circle, Shape::Square]);
/// assert_eq!(Shape::Square.id(), "square");
/// assert_eq!(Shape::from_id("circle"), Some(Shape::Circle));
/// assert_eq!(Shape::from_id("triangle"), None);
/// ```
#[macro_export]
macro_rules! stable_ids {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $id:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: [Self; [$($crate::stable_ids!(@unit $variant)),+].len()] =
                [$(Self::$variant),+];

            /// Stable identifier, used in settings files and actions.
            pub fn id(self) -> &'static str {
                match self {
                    $(Self::$variant => $id,)+
                }
            }

            /// The variant with stable identifier `id`, if any.
            pub fn from_id(id: &str) -> Option<Self> {
                Self::ALL.into_iter().find(|variant| variant.id() == id)
            }
        }
    };
    (@unit $variant:ident) => {
        ()
    };
}

#[cfg(test)]
mod tests {
    crate::stable_ids! {
        /// A test enum.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        enum Fruit {
            #[default]
            Apple => "apple",
            /// Documented variant.
            Banana => "banana",
            Cherry => "cherry",
        }
    }

    #[test]
    fn all_lists_every_variant_in_order() {
        assert_eq!(Fruit::ALL, [Fruit::Apple, Fruit::Banana, Fruit::Cherry]);
        assert_eq!(Fruit::default(), Fruit::Apple, "attributes are kept");
    }

    #[test]
    fn ids_round_trip_and_unknown_ids_are_none() {
        for fruit in Fruit::ALL {
            assert_eq!(Fruit::from_id(fruit.id()), Some(fruit));
        }
        assert_eq!(Fruit::Banana.id(), "banana");
        assert_eq!(Fruit::from_id("durian"), None);
    }
}
