// Keep wire assignments separate from Rust enum discriminants: these enums
// preserve unknown values and must be encoded through as_raw(), never a cast.
macro_rules! wire_code_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident: $wire:ty {
            $($(#[$variant_meta:meta])* $variant:ident = $value:literal,)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, strum_macros::EnumIter)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum $name {
            $($(#[$variant_meta])* $variant,)*
            /// A wire value without a named variant. All bits are preserved.
            #[strum(disabled)]
            NonStandard($wire),
        }

        impl $name {
            /// Decode a wire value, preserving values without a named variant.
            pub const fn from_raw(value: $wire) -> Self {
                match value {
                    $($value => Self::$variant,)*
                    other => Self::NonStandard(other),
                }
            }

            /// Return the exact wire value, including any sub-function flags.
            pub const fn as_raw(self) -> $wire {
                match self {
                    $(Self::$variant => $value,)*
                    Self::NonStandard(value) => value,
                }
            }
        }

        impl From<$wire> for $name {
            fn from(value: $wire) -> Self {
                Self::from_raw(value)
            }
        }

        impl From<$name> for $wire {
            fn from(value: $name) -> Self {
                value.as_raw()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.as_raw(), f)
            }
        }

        impl std::fmt::LowerHex for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::LowerHex::fmt(&self.as_raw(), f)
            }
        }

        impl std::fmt::UpperHex for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::UpperHex::fmt(&self.as_raw(), f)
            }
        }
    };
}
